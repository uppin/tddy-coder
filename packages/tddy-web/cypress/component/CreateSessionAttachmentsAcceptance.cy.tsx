/**
 * Acceptance tests: the new-session form (CreateSessionPane) lets an operator attach documents at
 * creation time, before the session exists.
 *
 * Local files are staged to the daemon the form's client is connected to — **on submit**, so nothing
 * is uploaded for a form that is abandoned — and referenced from `StartSessionRequest.attachments`
 * as a `StagedAttachmentRef`. The staged ref carries the *staging* host, which may differ from the
 * host that runs the session; the session host then fetches the bytes.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-session-attach-ui.md
 * Feature: docs/ft/coder/session-attachments.md
 */

import React from "react";
import { Room } from "livekit-client";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import {
  ConnectionService,
  StartSessionEventSchema,
  type StartSessionRequest,
} from "../../src/gen/connection_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { createSessionPage } from "../support/pages/createSessionPage";
import { branchConflictDialogPage } from "../support/pages/branchConflictDialogPage";
import { dragOverWith, dropFilesOnto, aFile } from "../support/util/fileDrop";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const LOCAL_HOST = "workstation-1";
const REMOTE_HOST = "server-2";

/** Branch another session already owns, so the first creation attempt is refused. */
const OWNED_BRANCH = "feat/auth";
const FREE_BRANCH = "feat/auth-rewrite";

/** 8 MiB — comfortably above the cap the cap-refusal test advertises. */
const EIGHT_MIB = 8 * 1024 * 1024;

const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)", maxAttachmentBytes: EIGHT_MIB },
  { instanceId: REMOTE_HOST, label: "server-2 (this daemon)", maxAttachmentBytes: EIGHT_MIB },
];

function aFilePick(fileName: string, contents: string): Cypress.FileReferenceObject {
  return { contents: Cypress.Buffer.from(contents), fileName, mimeType: "text/plain" };
}

/** Concatenates a file's staged chunks back into its text, so a test can prove nothing was lost. */
function reassembled(chunks: { data: Uint8Array }[]): string {
  const total = chunks.reduce((n, c) => n + c.data.length, 0);
  const joined = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    joined.set(chunk.data, offset);
    offset += chunk.data.length;
  }
  return new TextDecoder().decode(joined);
}

/**
 * Requests seen by `StreamStartSession`. The in-memory testkit records unary calls only, so a
 * streaming request has to be captured in a closure — see `cypress/support/rpc/acpReplay.ts` for the
 * same idiom.
 */
interface StartRecorder {
  requests: StartSessionRequest[];
}

/** Every RPC the form issues except the session start itself, which each test decides. */
function anAttachmentBackendWithoutStart(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(ConnectionService.method.listAgents, () => ({
      agents: [{ id: "claude", label: "Claude" }],
    }))
    .onUnary(ConnectionService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({
      branches: ["origin/main"],
      defaultRemote: "origin",
    }))
    .onUnary(ConnectionService.method.uploadStagedAttachmentChunk, (req) => ({
      entry: req.last
        ? {
            daemonInstanceId: req.daemonInstanceId || LOCAL_HOST,
            stagingId: req.stagingId,
            fileName: req.fileName,
            hostPath: `/srv/staging/${req.stagingId}/${req.fileName}`,
            sizeBytes: 0n,
            stagedAtMs: 0n,
          }
        : undefined,
    }))
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "attach-1" }));
}

/** A backend seeded with every RPC the form issues, plus staging and session start. */
function anAttachmentBackend(recorder: StartRecorder = { requests: [] }): InMemoryRpcBackend {
  // A form carrying attachments starts the session over the streaming RPC, so the host reports
  // materialization progress. Implemented via `.implement()` because `onUnary` is typed to unary
  // methods.
  return anAttachmentBackendWithoutStart().implement(ConnectionService, {
    async *streamStartSession(req: StartSessionRequest) {
      recorder.requests.push(req);
      for (const attachment of req.attachments) {
        yield create(StartSessionEventSchema, {
          event: {
            case: "attachmentProgress",
            value: {
              basename: attachment.basename,
              attachmentIndex: 0,
              attachmentCount: req.attachments.length,
              bytesDone: 0n,
              bytesTotal: 0n,
            },
          },
        });
      }
      yield create(StartSessionEventSchema, {
        event: { case: "result", value: { sessionId: "attach-1" } },
      });
    },
  });
}

/**
 * A backend that refuses the first streamed start with a branch conflict — nothing is created, and
 * the operator is asked how to proceed — and creates the session on the next one. The refusal is a
 * populated response field rather than an RPC error, which is what `CreateSessionBranchConflictAccep-
 * tance.cy.tsx` drives through the unary path.
 */
function aBackendRefusingTheFirstStartAsABranchConflict(
  recorder: StartRecorder,
): InMemoryRpcBackend {
  let starts = 0;
  return anAttachmentBackendWithoutStart().implement(ConnectionService, {
    async *streamStartSession(req: StartSessionRequest) {
      recorder.requests.push(req);
      starts += 1;
      const conflict = {
        sessionId: "",
        branchConflict: {
          branch: OWNED_BRANCH,
          owner: { exists: true, sessionId: "owner-session-1", isActive: true, status: "active" },
          suggestedBranchName: `${OWNED_BRANCH}-1`,
        },
      };
      yield create(StartSessionEventSchema, {
        event: { case: "result", value: starts === 1 ? conflict : { sessionId: "attach-1" } },
      });
    },
  });
}

function mountCreatePane(backend: InMemoryRpcBackend, onCreated = cy.stub().as("onCreated")) {
  const client = createClient(ConnectionService, backend.transport());
  cy.mount(
    <SelectedDaemonProvider room={new Room()} daemons={DAEMON_HOSTS} servingInstanceId={LOCAL_HOST}>
      <CreateSessionPane
        client={client}
        sessionToken="fake-token"
        onCancel={cy.stub()}
        onCreated={onCreated}
      />
    </SelectedDaemonProvider>,
  );
}

/** Fill the fields a tool session needs so Create is enabled. */
function fillRequiredFields() {
  createSessionPage.selectProject("proj-1");
  createSessionPage.selectAgent("claude");
}

/** The single streamed start request the form sent. */
function sentStartRequest(recorder: StartRecorder): StartSessionRequest {
  expect(recorder.requests, "exactly one StreamStartSession must have been sent").to.have.length(1);
  return recorder.requests[0]!;
}

/** Every staging chunk the form uploaded, across all of its submit attempts. */
function stagedChunks(backend: InMemoryRpcBackend) {
  return backend.callsTo(ConnectionService.method.uploadStagedAttachmentChunk);
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 900);
  cy.clearAllSessionStorage();
});

// ---------------------------------------------------------------------------
// Picking and listing
// ---------------------------------------------------------------------------

it("lists a picked file as an attachment row showing its size", () => {
  // Given a new-session form
  mountCreatePane(anAttachmentBackend());

  // When a local file is picked
  createSessionPage.pickFiles([aFilePick("spec.md", "# spec body")]);

  // Then it is listed by basename, with the byte count the host will store
  createSessionPage.attachmentBasenames().should("deep.equal", ["spec.md"]);
  createSessionPage.attachmentSize("spec.md").should("have.text", "11 B");
});

it("shows a drop overlay while a file drag is over the attachments section", () => {
  // Given a new-session form with nothing attached
  mountCreatePane(anAttachmentBackend());
  createSessionPage.attachmentDropOverlay().should("not.exist");

  // When a file is dragged over the section
  dragOverWith(createSessionPage.attachmentDropSelector, [aFile("notes.md", "hi")]);

  // Then the drop target is indicated
  createSessionPage.attachmentDropOverlay().should("be.visible");
});

it("lists a dropped file the same way a picked one is listed", () => {
  // Given a new-session form
  mountCreatePane(anAttachmentBackend());

  // When two files are dropped
  dropFilesOnto(createSessionPage.attachmentDropSelector, [
    aFile("alpha.md", "AAAA"),
    aFile("beta.md", "BBBBBB"),
  ]);

  // Then both are listed, in drop order
  createSessionPage.attachmentBasenames().should("deep.equal", ["alpha.md", "beta.md"]);
});

it("removes an attachment before the session is created", () => {
  // Given two picked files
  mountCreatePane(anAttachmentBackend());
  createSessionPage.pickFiles([aFilePick("keep.md", "keep"), aFilePick("drop.md", "drop")]);

  // When one is removed
  createSessionPage.removeAttachment("drop.md");

  // Then only the other remains
  createSessionPage.attachmentBasenames().should("deep.equal", ["keep.md"]);
});

// ---------------------------------------------------------------------------
// Nothing is uploaded until submit
// ---------------------------------------------------------------------------

it("uploads nothing until the form is submitted", () => {
  // Given a picked file on a form that is never submitted
  const backend = anAttachmentBackend();
  mountCreatePane(backend);
  createSessionPage.pickFiles([aFilePick("spec.md", "# spec body")]);

  // When / Then — the row exists, but no staging chunk was sent
  createSessionPage.attachmentRow("spec.md").should("exist");
  cy.wrap(null).should(() => {
    expect(
      backend.callsTo(ConnectionService.method.uploadStagedAttachmentChunk),
      "an abandoned form must upload nothing",
    ).to.have.length(0);
  });
});

it("stages a picked file on submit and references it from the start request", () => {
  // Given a picked file and a complete form
  const recorder: StartRecorder = { requests: [] };
  const backend = anAttachmentBackend(recorder);
  mountCreatePane(backend);
  fillRequiredFields();
  createSessionPage.pickFiles([aFilePick("spec.md", "# spec body")]);

  // When the session is created
  createSessionPage.submit();

  // Then the bytes were staged, whole, under one staging id
  cy.get("@onCreated").should("have.been.calledWith", "attach-1");
  cy.wrap(null).should(() => {
    const chunks = backend.callsTo(ConnectionService.method.uploadStagedAttachmentChunk);
    const stagingIds = new Set(chunks.map((c) => c.stagingId));
    expect(stagingIds.size, "all chunks of one submit share a staging id").to.equal(1);
    const stagingId = [...stagingIds][0]!;
    // A UUID minted at submit time, so only its presence can be asserted — the exact value is not
    // knowable from outside the form.
    expect(stagingId, "staging id must be non-empty").to.have.length.greaterThan(0);
    expect(
      chunks.filter((c) => c.last),
      "exactly one chunk closes the file",
    ).to.have.length(1);
    expect(reassembled(chunks), "the file's bytes arrive intact").to.equal("# spec body");

    // And the start request references it as a staged attachment
    const attachments = sentStartRequest(recorder).attachments;
    expect(attachments).to.have.length(1);
    expect(attachments[0]!.basename).to.equal("spec.md");
    expect(attachments[0]!.source.case).to.equal("staged");
    expect(attachments[0]!.source.value).to.include({
      daemonInstanceId: LOCAL_HOST,
      stagingId,
      fileName: "spec.md",
    });
  });
});

// ---------------------------------------------------------------------------
// Retrying a refused creation
// ---------------------------------------------------------------------------

it("reuses the staged bytes when a refused creation is retried, uploading nothing a second time", () => {
  // Given a picked file whose first creation is refused because another session owns the branch
  const recorder: StartRecorder = { requests: [] };
  const backend = aBackendRefusingTheFirstStartAsABranchConflict(recorder);
  mountCreatePane(backend);
  fillRequiredFields();
  createSessionPage.typeNewBranchName(OWNED_BRANCH);
  createSessionPage.pickFiles([aFilePick("spec.md", "# spec body")]);
  createSessionPage.submit();
  branchConflictDialogPage.dialog().should("be.visible");

  // When the operator answers the prompt by naming a free branch, which re-runs the creation
  branchConflictDialogPage.renameTo(FREE_BRANCH);

  // Then the second attempt creates the session by referencing the batch already on the host — a
  // re-upload would mean re-sending the whole file over the data channel for every retry
  cy.get("@onCreated").should("have.been.calledWith", "attach-1");
  cy.wrap(null).should(() => {
    expect(recorder.requests, "the creation was attempted twice").to.have.length(2);
    const chunks = stagedChunks(backend);
    expect(chunks, "the file's bytes are uploaded once, not once per attempt").to.have.length(1);
    const retried = recorder.requests[1]!;
    expect(retried.newBranchName, "the retry names the free branch").to.equal(FREE_BRANCH);
    expect(retried.attachments).to.have.length(1);
    expect(retried.attachments[0]!.source.value).to.include({
      daemonInstanceId: LOCAL_HOST,
      stagingId: chunks[0]!.stagingId,
      fileName: "spec.md",
    });
  });
});

// ---------------------------------------------------------------------------
// Renaming
// ---------------------------------------------------------------------------

it("sends a renamed attachment under the new basename with the original source file name", () => {
  // Given a picked file that the operator renames
  const recorder: StartRecorder = { requests: [] };
  mountCreatePane(anAttachmentBackend(recorder));
  fillRequiredFields();
  createSessionPage.pickFiles([aFilePick("scan001.pdf", "pdf bytes")]);
  createSessionPage.renameAttachment("scan001.pdf", "requirements.pdf");

  // When the session is created
  createSessionPage.submit();

  // Then basename carries the new name while the staged file keeps the name it was uploaded under
  cy.wrap(null).should(() => {
    const attachments = sentStartRequest(recorder).attachments;
    expect(attachments).to.have.length(1);
    expect(attachments[0]!.basename).to.equal("requirements.pdf");
    expect(attachments[0]!.source.case).to.equal("staged");
    expect(attachments[0]!.source.value!.fileName).to.equal("scan001.pdf");
  });
});

it("refuses a rename that collides with another attachment, before submitting", () => {
  // Given two picked files
  const recorder: StartRecorder = { requests: [] };
  mountCreatePane(anAttachmentBackend(recorder));
  fillRequiredFields();
  createSessionPage.pickFiles([aFilePick("a.md", "A"), aFilePick("b.md", "B")]);

  // When one is renamed onto the other's name
  createSessionPage.renameAttachment("b.md", "a.md");

  // Then the collision is named inline and nothing is sent
  createSessionPage.attachmentError().should("contain.text", "a.md");
  cy.wrap(null).should(() => {
    expect(recorder.requests, "a duplicate basename must not reach the daemon").to.have.length(0);
  });
});

it("refuses a rename to a name that is not a single path segment", () => {
  // Given a picked file
  mountCreatePane(anAttachmentBackend());
  createSessionPage.pickFiles([aFilePick("spec.md", "# spec")]);

  // When it is renamed to a path
  createSessionPage.renameAttachment("spec.md", "docs/spec.md");

  // Then the refusal is shown inline
  createSessionPage.attachmentError().should("be.visible");
});

// ---------------------------------------------------------------------------
// The host's advertised cap
// ---------------------------------------------------------------------------

it("refuses a file larger than the selected host's advertised cap, naming the limit", () => {
  // Given a form whose selected host advertises an 8 MiB cap, and a 9 MiB file
  mountCreatePane(anAttachmentBackend());

  // When the oversized file is picked
  createSessionPage.pickFiles([
    { contents: Cypress.Buffer.alloc(9 * 1024 * 1024, 0), fileName: "big.bin" },
  ]);

  // Then it is refused at pick time, and never becomes a row
  createSessionPage.attachmentError().should("contain.text", "8 MiB");
  createSessionPage.attachmentRow("big.bin").should("not.exist");
});

// ---------------------------------------------------------------------------
// Staging host vs session host
// ---------------------------------------------------------------------------

it("stamps the staged ref with the staging host even when the session runs elsewhere", () => {
  // Given a form connected to one daemon but starting the session on another
  const recorder: StartRecorder = { requests: [] };
  const backend = anAttachmentBackend(recorder);
  mountCreatePane(backend);
  fillRequiredFields();
  createSessionPage.selectHost(REMOTE_HOST);
  createSessionPage.pickFiles([aFilePick("spec.md", "# spec")]);

  // When the session is created
  createSessionPage.submit();

  // Then the session targets the remote host while the staged ref names the host that holds the
  // bytes — the session host fetches them from there
  cy.wrap(null).should(() => {
    const request = sentStartRequest(recorder);
    expect(request.daemonInstanceId, "the session runs on the chosen host").to.equal(REMOTE_HOST);
    const staged = request.attachments[0]!.source.value!;
    expect(staged.daemonInstanceId, "the staged ref names the staging host").to.equal(LOCAL_HOST);

    // And the upload itself went to the staging host, not the session host
    const chunks = backend.callsTo(ConnectionService.method.uploadStagedAttachmentChunk);
    expect(new Set(chunks.map((c) => c.daemonInstanceId))).to.deep.equal(new Set([LOCAL_HOST]));
  });
});
