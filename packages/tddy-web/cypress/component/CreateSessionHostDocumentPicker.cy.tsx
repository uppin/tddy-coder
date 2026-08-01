/**
 * Acceptance tests: the new-session form can attach a document that already exists on a host, by
 * reference, with no upload.
 *
 * The picker browses the host by `HostDocumentScope` and produces a `HostDocumentRef` — scope plus a
 * validated relative path, never an absolute host path, so a client cannot name an arbitrary file the
 * daemon's user happens to be able to read.
 *
 * One trap this pins: a session artifact's `relative_path` is **not** the doc's basename. An
 * attachment-kind context doc lives at `attachments/<basename>` under `artifacts/`, so the ref must
 * carry that prefix while a manifest-kind doc must not. Getting this wrong is silent — the daemon
 * simply reports the document as missing.
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
  HostDocumentScope,
  SessionContextDocKind,
  StartSessionEventSchema,
  type StartSessionRequest,
} from "../../src/gen/connection_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { createSessionPage } from "../support/pages/createSessionPage";

const LOCAL_HOST = "workstation-1";
const OWNING_SESSION = "sess-owning-1";

/** 8 MiB — the cap the oversized-document test picks against. */
const EIGHT_MIB = 8 * 1024 * 1024;

const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)", maxAttachmentBytes: EIGHT_MIB },
];

interface StartRecorder {
  requests: StartSessionRequest[];
}

/**
 * A backend whose host holds one session with two context docs — a recipe-owned manifest doc and a
 * user-attached one — plus one uploaded file and one worktree file.
 */
function aHostWithDocuments(recorder: StartRecorder): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessions, () => ({
      sessions: [
        {
          sessionId: OWNING_SESSION,
          // The worktree scope's root is this session's worktree, so the picker needs its path to
          // list it — `repo_path` is what `SessionEntry` carries it as.
          repoPath: `/srv/worktrees/${OWNING_SESSION}`,
          projectId: "proj-1",
          contextDocs: [
            {
              key: "PRD",
              basename: "PRD.md",
              path: `/srv/sessions/${OWNING_SESSION}/artifacts/PRD.md`,
              description: "Product requirements",
              exists: true,
              kind: SessionContextDocKind.MANIFEST,
              sizeBytes: 120n,
            },
            {
              key: "screenshot.png",
              basename: "screenshot.png",
              path: `/srv/sessions/${OWNING_SESSION}/artifacts/attachments/screenshot.png`,
              description: "Attached document",
              exists: true,
              kind: SessionContextDocKind.ATTACHMENT,
              sizeBytes: 2048n,
            },
            {
              key: "exploration",
              basename: "exploration.md",
              path: `/srv/sessions/${OWNING_SESSION}/artifacts/exploration.md`,
              description: "Never written",
              exists: false,
              kind: SessionContextDocKind.MANIFEST,
              sizeBytes: 0n,
            },
          ],
        },
      ],
    }))
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
    .onUnary(ConnectionService.method.listSessionUploads, () => ({
      uploads: [
        {
          uploadId: "up-1",
          fileName: "trace.log",
          hostPath: `/srv/sessions/${OWNING_SESSION}/uploads/up-1/trace.log`,
          sizeBytes: 4096n,
          uploadedAtMs: 0n,
        },
      ],
    }))
    .implement(ConnectionService, {
      async *streamStartSession(req: StartSessionRequest) {
        recorder.requests.push(req);
        yield create(StartSessionEventSchema, {
          event: { case: "result", value: { sessionId: "host-doc-1" } },
        });
      },
    });
}

function mountCreatePane(backend: InMemoryRpcBackend) {
  const client = createClient(ConnectionService, backend.transport());
  cy.mount(
    <SelectedDaemonProvider room={new Room()} daemons={DAEMON_HOSTS} servingInstanceId={LOCAL_HOST}>
      <CreateSessionPane
        client={client}
        sessionToken="fake-token"
        onCancel={cy.stub()}
        onCreated={cy.stub().as("onCreated")}
      />
    </SelectedDaemonProvider>,
  );
}

function fillRequiredFields() {
  createSessionPage.selectProject("proj-1");
  createSessionPage.selectAgent("claude");
}

/** The one source the form sent, asserted to be a host-document reference. */
function sentHostDocumentRef(recorder: StartRecorder) {
  expect(recorder.requests, "exactly one start request must have been sent").to.have.length(1);
  const attachments = recorder.requests[0]!.attachments;
  expect(attachments, "exactly one attachment").to.have.length(1);
  expect(attachments[0]!.source.case, "attached by reference, not staged").to.equal("hostDocument");
  return attachments[0]!;
}

beforeEach(() => {
  cy.viewport(1280, 900);
  cy.clearAllSessionStorage();
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("attaches a session artifact by reference without uploading anything", () => {
  // Given a host holding a session whose recipe wrote a PRD
  const recorder: StartRecorder = { requests: [] };
  const backend = aHostWithDocuments(recorder);
  mountCreatePane(backend);
  fillRequiredFields();

  // When that document is picked from the host
  createSessionPage.openHostDocPicker();
  createSessionPage.selectHostDocScope(String(HostDocumentScope.SESSION_ARTIFACT));
  createSessionPage.pickHostDoc("PRD.md");
  createSessionPage.submit();

  // Then it is referenced, not staged, and no bytes were uploaded
  cy.get("@onCreated").should("have.been.calledWith", "host-doc-1");
  cy.wrap(null).should(() => {
    const attachment = sentHostDocumentRef(recorder);
    expect(attachment.basename).to.equal("PRD.md");
    expect(attachment.source.value).to.include({
      daemonInstanceId: LOCAL_HOST,
      scope: HostDocumentScope.SESSION_ARTIFACT,
      sessionId: OWNING_SESSION,
      relativePath: "PRD.md",
    });
    expect(
      backend.callsTo(ConnectionService.method.uploadStagedAttachmentChunk),
      "a referenced document uploads nothing",
    ).to.have.length(0);
  });
});

it("prefixes an attachment-kind artifact's path with the attachments directory", () => {
  // Given a host holding a session with a user-attached screenshot
  const recorder: StartRecorder = { requests: [] };
  mountCreatePane(aHostWithDocuments(recorder));
  fillRequiredFields();

  // When that attachment is picked
  createSessionPage.openHostDocPicker();
  createSessionPage.selectHostDocScope(String(HostDocumentScope.SESSION_ARTIFACT));
  createSessionPage.pickHostDoc("attachments/screenshot.png");
  createSessionPage.submit();

  // Then the ref names it under attachments/, while its basename stays the bare file name
  cy.wrap(null).should(() => {
    const attachment = sentHostDocumentRef(recorder);
    expect(attachment.basename).to.equal("screenshot.png");
    expect(attachment.source.value!.relativePath).to.equal("attachments/screenshot.png");
  });
});

it("does not offer a manifest doc the recipe never wrote", () => {
  // Given a host whose session declares a doc that does not exist on disk
  mountCreatePane(aHostWithDocuments({ requests: [] }));

  // When the artifact scope is browsed
  createSessionPage.openHostDocPicker();
  createSessionPage.selectHostDocScope(String(HostDocumentScope.SESSION_ARTIFACT));

  // Then the written docs are offered and the unwritten one is not — picking it would only earn a
  // NOT_FOUND from the daemon
  createSessionPage.hostDocPicker().should("be.visible");
  createSessionPage.hostDocRow("PRD.md").should("exist");
  createSessionPage.hostDocRow("exploration.md").should("not.exist");
});

it("attaches an uploaded file by its upload id and file name", () => {
  // Given a host holding a session with an uploaded log
  const recorder: StartRecorder = { requests: [] };
  mountCreatePane(aHostWithDocuments(recorder));
  fillRequiredFields();

  // When it is picked from the uploads scope
  createSessionPage.openHostDocPicker();
  createSessionPage.selectHostDocScope(String(HostDocumentScope.SESSION_UPLOAD));
  createSessionPage.pickHostDoc("up-1/trace.log");
  createSessionPage.submit();

  // Then the two-segment path the daemon requires is what gets sent
  cy.wrap(null).should(() => {
    const attachment = sentHostDocumentRef(recorder);
    expect(attachment.basename).to.equal("trace.log");
    expect(attachment.source.value).to.include({
      scope: HostDocumentScope.SESSION_UPLOAD,
      sessionId: OWNING_SESSION,
      relativePath: "up-1/trace.log",
    });
  });
});

// ---------------------------------------------------------------------------
// Tree-browsing scopes — the session's worktree and the project's repo
// ---------------------------------------------------------------------------

/**
 * These two scopes browse a real directory tree rather than a flat list, so the picker reuses
 * `WorktreeFileTree` and `worktreeFilesApi` — both already know how to list one directory at a time.
 * The project-repo scope lists the project's **primary** worktree by its `main_repo_path`, which
 * `ListWorktreeDirectory` accepts (pinned host-side by
 * `worktree_files_rpc.rs::list_worktree_directory_lists_the_projects_primary_worktree`).
 */
function aHostWithATree(recorder: StartRecorder): InMemoryRpcBackend {
  return aHostWithDocuments(recorder).onUnary(
    ConnectionService.method.listWorktreeDirectory,
    (req) =>
      req.relPath === ""
        ? {
            entries: [
              { name: "docs", isDir: true, sizeBytes: 0n },
              { name: "README.md", isDir: false, sizeBytes: 24n },
              { name: "huge.bin", isDir: false, sizeBytes: 9n * 1024n * 1024n },
            ],
          }
        : { entries: [{ name: "spec.md", isDir: false, sizeBytes: 64n }] },
  );
}

it("attaches a project-repo file by reference, carrying the project id and no session id", () => {
  // Given a host whose project repo holds a checked-in README
  const recorder: StartRecorder = { requests: [] };
  const backend = aHostWithATree(recorder);
  mountCreatePane(backend);
  fillRequiredFields();

  // When it is picked from the project-repo scope
  createSessionPage.openHostDocPicker();
  createSessionPage.selectHostDocScope(String(HostDocumentScope.PROJECT_REPO));
  createSessionPage.pickHostDocFromTree("README.md");
  createSessionPage.submit();

  // Then the ref names the project, not a session — the scope root is the project's repo
  cy.get("@onCreated").should("have.been.calledWith", "host-doc-1");
  cy.wrap(null).should(() => {
    const attachment = sentHostDocumentRef(recorder);
    expect(attachment.basename).to.equal("README.md");
    expect(attachment.source.value).to.include({
      scope: HostDocumentScope.PROJECT_REPO,
      projectId: "proj-1",
      sessionId: "",
      relativePath: "README.md",
    });
    expect(
      backend.callsTo(ConnectionService.method.uploadStagedAttachmentChunk),
      "a referenced document uploads nothing",
    ).to.have.length(0);
  });
});

it("attaches a session-worktree file by reference, carrying the session id and no project id", () => {
  // Given a host holding a session with a worktree
  const recorder: StartRecorder = { requests: [] };
  mountCreatePane(aHostWithATree(recorder));
  fillRequiredFields();

  // When a working-copy file is picked
  createSessionPage.openHostDocPicker();
  createSessionPage.selectHostDocScope(String(HostDocumentScope.SESSION_WORKTREE));
  createSessionPage.pickHostDocFromTree("README.md");
  createSessionPage.submit();

  // Then the ref names the owning session — the scope root is that session's worktree
  cy.wrap(null).should(() => {
    const attachment = sentHostDocumentRef(recorder);
    expect(attachment.source.value).to.include({
      scope: HostDocumentScope.SESSION_WORKTREE,
      sessionId: OWNING_SESSION,
      projectId: "",
      relativePath: "README.md",
    });
  });
});

it("attaches a file from inside a subdirectory under its full relative path", () => {
  // Given a project repo with a docs/ directory
  const recorder: StartRecorder = { requests: [] };
  mountCreatePane(aHostWithATree(recorder));
  fillRequiredFields();
  createSessionPage.openHostDocPicker();
  createSessionPage.selectHostDocScope(String(HostDocumentScope.PROJECT_REPO));

  // When the directory is expanded and the file inside it picked
  createSessionPage.expandHostDocDir("docs");
  createSessionPage.pickHostDocFromTree("docs/spec.md");
  createSessionPage.submit();

  // Then the ref carries the path relative to the scope root, while basename stays the file name —
  // a ref that dropped the directory would resolve to a file that does not exist
  cy.wrap(null).should(() => {
    const attachment = sentHostDocumentRef(recorder);
    expect(attachment.basename).to.equal("spec.md");
    expect(attachment.source.value!.relativePath).to.equal("docs/spec.md");
  });
});

it("refuses a host document over the host's advertised cap, naming the limit", () => {
  // Given a project repo holding a 9 MiB file, on a host advertising an 8 MiB cap
  mountCreatePane(aHostWithATree({ requests: [] }));
  createSessionPage.openHostDocPicker();
  createSessionPage.selectHostDocScope(String(HostDocumentScope.PROJECT_REPO));

  // When the oversized file is picked
  createSessionPage.pickHostDocFromTree("huge.bin");

  // Then it is refused at pick time and never becomes an attachment row — the size is known from the
  // listing, so there is no reason to let the host refuse it after the upload-free reference is sent
  createSessionPage.attachmentError().should("contain.text", "8 MiB");
  createSessionPage.attachmentRow("huge.bin").should("not.exist");
});
