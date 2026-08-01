/**
 * Acceptance tests: while a session with attachments is being created, the host reports what it is
 * doing and the form renders it per attachment row.
 *
 * Progress comes from `StreamStartSession` — the host is the only party that knows how far
 * materialization has got, especially when the bytes are being fetched from another host. The stream
 * ends with exactly one terminal result, which is what creates the session.
 *
 * Mid-stream state is asserted on `data-attachment-percent` rather than rendered text, and the stub
 * generator is held at a gate so the value is exact rather than whatever the race settled on — the
 * same technique as `TerminalFileUploadProgressFooter.cy.tsx`.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-session-attach-ui.md
 */

import React from "react";
import { Room } from "livekit-client";
import { createClient, ConnectError, Code } from "@connectrpc/connect";
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

const LOCAL_HOST = "workstation-1";
const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)" },
];

function aFilePick(fileName: string, contents: string): Cypress.FileReferenceObject {
  return { contents: Cypress.Buffer.from(contents), fileName, mimeType: "text/plain" };
}

/** Every RPC the form issues besides the session start itself. */
function aBaselineBackend(): InMemoryRpcBackend {
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
            daemonInstanceId: LOCAL_HOST,
            stagingId: req.stagingId,
            fileName: req.fileName,
            hostPath: `/srv/staging/${req.stagingId}/${req.fileName}`,
            sizeBytes: 0n,
            stagedAtMs: 0n,
          }
        : undefined,
    }));
}

interface HeldStream {
  backend: InMemoryRpcBackend;
  opens: { count: number };
  releaseResult: () => void;
}

/**
 * A backend whose start stream emits one half-done progress event for `basename`, then **holds** —
 * so the form's mid-flight rendering is a settled, exact state — and finishes on `releaseResult()`.
 */
function aBackendHoldingProgressAt(basename: string, percentDone: number): HeldStream {
  const opens = { count: 0 };
  let release: () => void = () => undefined;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const backend = aBaselineBackend().implement(ConnectionService, {
    async *streamStartSession(req: StartSessionRequest) {
      opens.count += 1;
      const total = 1000n;
      yield create(StartSessionEventSchema, {
        event: {
          case: "attachmentProgress",
          value: {
            basename,
            attachmentIndex: 0,
            attachmentCount: req.attachments.length,
            bytesDone: (BigInt(percentDone) * total) / 100n,
            bytesTotal: total,
          },
        },
      });
      await held;
      yield create(StartSessionEventSchema, {
        event: { case: "result", value: { sessionId: "progress-1" } },
      });
    },
  });
  return { backend, opens, releaseResult: () => release() };
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

function submitWithOneAttachment() {
  createSessionPage.selectProject("proj-1");
  createSessionPage.selectAgent("claude");
  createSessionPage.pickFiles([aFilePick("spec.md", "# spec body")]);
  createSessionPage.submit();
}

beforeEach(() => {
  cy.viewport(1280, 900);
  cy.clearAllSessionStorage();
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("renders the host's reported progress on the attachment's own row", () => {
  // Given a host that reports one attachment 40% materialized, then holds
  const { backend, opens, releaseResult } = aBackendHoldingProgressAt("spec.md", 40);
  mountCreatePane(backend);

  // When the session is created
  submitWithOneAttachment();

  // Then that row shows exactly the reported percentage while the stream is still open
  cy.wrap(opens).its("count").should("equal", 1);
  createSessionPage
    .attachmentProgress("spec.md")
    .should("have.attr", "data-attachment-percent", "40");

  // And the session is created once the host reports its result
  cy.then(() => releaseResult());
  cy.get("@onCreated").should("have.been.calledWith", "progress-1");
});

it("keeps Create disabled while the host is still materializing", () => {
  // Given a host that has reported progress but not finished
  const { backend, opens, releaseResult } = aBackendHoldingProgressAt("spec.md", 40);
  mountCreatePane(backend);

  // When the session is created
  submitWithOneAttachment();
  cy.wrap(opens).its("count").should("equal", 1);

  // Then Create cannot be pressed again mid-flight
  createSessionPage.submitButton().should("be.disabled");

  // And it is the terminal result, not the first progress event, that completes the creation
  cy.get("@onCreated").should("not.have.been.called");
  cy.then(() => releaseResult());
  cy.get("@onCreated").should("have.been.calledWith", "progress-1");
});

it("surfaces a failed materialization as a creation error and creates no session", () => {
  // Given a host that reports progress and then fails the stream
  const backend = aBaselineBackend().implement(ConnectionService, {
    async *streamStartSession(req: StartSessionRequest) {
      yield create(StartSessionEventSchema, {
        event: {
          case: "attachmentProgress",
          value: {
            basename: "spec.md",
            attachmentIndex: 0,
            attachmentCount: req.attachments.length,
            bytesDone: 0n,
            bytesTotal: 1000n,
          },
        },
      });
      throw new ConnectError("an attachment with this name already exists", Code.FailedPrecondition);
    },
  });
  mountCreatePane(backend);

  // When the session is created
  submitWithOneAttachment();

  // Then the failure is shown and nothing was created — a stream error is a failed creation, not
  // merely an interrupted progress bar
  createSessionPage
    .error()
    .should("contain.text", "an attachment with this name already exists");
  cy.get("@onCreated").should("not.have.been.called");
  createSessionPage.submitButton().should("not.be.disabled");
});

it("starts a session with no attachments over the unary RPC, leaving that path unchanged", () => {
  // Given a form with nothing attached, and a host whose streaming RPC would fail if used
  const backend = aBaselineBackend()
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "unary-1" }))
    .implement(ConnectionService, {
      async *streamStartSession() {
        throw new ConnectError("the streaming RPC must not be used without attachments", Code.Internal);
        yield create(StartSessionEventSchema, {});
      },
    });
  mountCreatePane(backend);

  // When the session is created without attaching anything
  createSessionPage.selectProject("proj-1");
  createSessionPage.selectAgent("claude");
  createSessionPage.submit();

  // Then the unary RPC created it
  cy.get("@onCreated").should("have.been.calledWith", "unary-1");
  cy.wrap(null).should(() => {
    expect(backend.callsTo(ConnectionService.method.startSession)).to.have.length(1);
  });
});
