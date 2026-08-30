/**
 * Acceptance: the Start-session dialog for a planned PR arrives with the orchestrator's documents
 * already attached.
 *
 * A child agent that never reads its own boundaries builds whatever it infers from a title and a
 * one-line description — and two children inferring the same abstraction is the duplicate work the
 * whole documents feature exists to prevent. So the dialog pre-populates the node's PRD and
 * changeset plus the two shared documents, rather than leaving the operator to find them in the
 * host-document picker every time.
 *
 * They are **rows, not an invariant**. An operator restarting an orphaned node whose child already
 * holds the documents should not be forced to re-attach them, so every row removes like any other —
 * and what is left attached at submit is exactly what is sent.
 *
 * Feature: docs/ft/coder/pr-stack-docs.md#auto-attachment-in-the-start-session-dialog
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { create } from "@bufbuild/protobuf";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ConnectionService,
  StartSessionEventSchema,
  type ProjectEntry,
  type StartSessionRequest,
} from "../../src/gen/connection_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend, type SessionEntryFixture } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { createSessionPage } from "../support/pages/createSessionPage";
import { aPlannedNode, aStackPlanJson } from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-7777-0000-0000-0000-000000000077";
const ORCHESTRATOR_HOST = "local";
const PROJECT_ID = "proj-pr-stack";

/** A documented node: the docs pass has run, so it owns a PRD and a changeset. */
const DOCUMENTED_NODE = aPlannedNode({
  nodeId: "n2",
  title: "Auth middleware",
  branchSuggestion: "feature/auth/middleware",
  parents: ["n1"],
});

/** The stack's root, already built — present so n2 has a real base to offer. */
const ROOT_NODE = aPlannedNode({
  nodeId: "n1",
  title: "Token store",
  branch: "feature/auth/token-store",
  sessionId: "session-token-store",
});

/** What the orchestrator holds on disk once `write-stack-docs` has run. */
const ORCHESTRATOR_CONTEXT_DOCS = [
  { basename: "PRD.md", relativePath: "prs/n2/PRD.md", exists: true },
  { basename: "changeset.md", relativePath: "prs/n2/changeset.md", exists: true },
  { basename: "pr-stack-plan.md", relativePath: "pr-stack-plan.md", exists: true },
  { basename: "exploration.md", relativePath: "exploration.md", exists: true },
];

/** The four documents, in the order the dialog lists them: the node's own pair, then the shared. */
const ALL_FOUR = ["PRD.md", "changeset.md", "pr-stack-plan.md", "exploration.md"];

const PROJECT: Partial<ProjectEntry> = {
  projectId: PROJECT_ID,
  name: "pr-stack-project",
  mainRepoPath: "/home/dev/pr-stack-project",
  mainBranchRef: "origin/master",
  daemonInstanceId: ORCHESTRATOR_HOST,
};

/**
 * The `StartSession` requests the form sent. Recorded in the streaming handler rather than read off
 * the backend's unary log: a creation carrying attachments goes out over `StreamStartSession`, which
 * is the only entry point that reports per-attachment materialization progress.
 */
interface StartRecorder {
  requests: StartSessionRequest[];
}

function anOrchestratorBackend(
  recorder: StartRecorder,
  contextDocs = ORCHESTRATOR_CONTEXT_DOCS,
): InMemoryRpcBackend {
  const orchestrator: SessionEntryFixture = {
    sessionId: ORCHESTRATOR_SESSION_ID,
    daemonInstanceId: ORCHESTRATOR_HOST,
    projectId: PROJECT_ID,
    repoPath: "/home/dev/pr-stack-project",
    sessionType: "tool",
    recipe: "pr-stack",
    stackPlanJson: aStackPlanJson(1, [ROOT_NODE, DOCUMENTED_NODE]),
    contextDocs,
  };
  return aSessionsDrawerBackend([orchestrator])
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listAgents, () => ({
      agents: [{ id: "claude", label: "Claude" }],
    }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({
      branches: [],
      defaultRemote: "origin",
    }))
    .implement(ConnectionService, {
      async *streamStartSession(req: StartSessionRequest) {
        recorder.requests.push(req);
        yield create(StartSessionEventSchema, {
          event: { case: "result", value: { sessionId: "child-n2" } },
        });
      },
    });
}

function aStartRecorder(): StartRecorder {
  return { requests: [] };
}

function openTheStartSessionDialogForTheDocumentedNode() {
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("n2").click();
}

/** The one `StartSession` the form sent, whose attachments are what the child will materialize. */
function sentAttachments(recorder: StartRecorder) {
  expect(recorder.requests, "exactly one start request must have been sent").to.have.length(1);
  return recorder.requests[0]!.attachments;
}

beforeEach(() => {
  cy.viewport(1280, 900);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("PR-stack start session — attached documents", () => {
  it("lists the node's PRD and changeset alongside the shared stack documents", () => {
    // Given
    mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), anOrchestratorBackend(aStartRecorder()));

    // When
    openTheStartSessionDialogForTheDocumentedNode();

    // Then
    createSessionPage.attachmentBasenames().should("deep.equal", ALL_FOUR);
  });

  it("attaches the changeset that belongs to the node being started", () => {
    // Given
    const recorder = aStartRecorder();
    mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), anOrchestratorBackend(recorder));
    openTheStartSessionDialogForTheDocumentedNode();

    // When
    createSessionPage.submitButton().click();

    // Then — sourced from n2's directory, not n1's. Attaching another node's boundaries would be
    // worse than attaching none, and the row itself only ever shows the flat destination name.
    cy.wrap(null).should(() => {
      const changeset = sentAttachments(recorder).find((a) => a.basename === "changeset.md");
      expect(changeset?.source.value).to.include({ relativePath: "prs/n2/changeset.md" });
    });
  });

  it("lists only the shared documents for a node the docs pass has not covered", () => {
    // Given — an orchestrator whose per-PR documents do not exist yet
    const sharedOnly = ORCHESTRATOR_CONTEXT_DOCS.filter(
      (doc) => !doc.relativePath.startsWith("prs/"),
    );
    mountWithRpc(
      withSelectedDaemon(<SessionsDrawerScreen />),
      anOrchestratorBackend(aStartRecorder(), sharedOnly),
    );

    // When — starting a node early is allowed, it just carries less context
    openTheStartSessionDialogForTheDocumentedNode();

    // Then
    createSessionPage
      .attachmentBasenames()
      .should("deep.equal", ["pr-stack-plan.md", "exploration.md"]);
  });

  it("lists no documents for an orchestrator that has written none", () => {
    // Given
    mountWithRpc(
      withSelectedDaemon(<SessionsDrawerScreen />),
      anOrchestratorBackend(aStartRecorder(), []),
    );

    // When
    openTheStartSessionDialogForTheDocumentedNode();

    // Then — no rows at all, and the dialog still starts a session
    createSessionPage.expectNoAttachments();
    createSessionPage.submitButton().should("be.enabled");
  });

  it("removes an auto-attached document when the operator drops it", () => {
    // Given
    mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), anOrchestratorBackend(aStartRecorder()));
    openTheStartSessionDialogForTheDocumentedNode();

    // When
    createSessionPage.removeAttachment("exploration.md");

    // Then
    createSessionPage
      .attachmentBasenames()
      .should("deep.equal", ["PRD.md", "changeset.md", "pr-stack-plan.md"]);
  });

  it("sends exactly the documents left attached when the session is started", () => {
    // Given
    const recorder = aStartRecorder();
    mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), anOrchestratorBackend(recorder));
    openTheStartSessionDialogForTheDocumentedNode();

    // When — the operator drops one document and starts the session
    createSessionPage.removeAttachment("exploration.md");
    createSessionPage.submitButton().click();

    // Then
    cy.wrap(null).should(() => {
      expect(sentAttachments(recorder).map((a) => a.basename)).to.deep.equal([
        "PRD.md",
        "changeset.md",
        "pr-stack-plan.md",
      ]);
    });
  });

  it("sends each document as a reference to the orchestrator's own session", () => {
    // Given
    const recorder = aStartRecorder();
    mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), anOrchestratorBackend(recorder));
    openTheStartSessionDialogForTheDocumentedNode();

    // When
    createSessionPage.submitButton().click();

    // Then — attached by reference, so nothing is uploaded and a cross-host stack works unchanged
    cy.wrap(null).should(() => {
      const attachments = sentAttachments(recorder);
      expect(attachments.map((a) => a.source.case)).to.deep.equal([
        "hostDocument",
        "hostDocument",
        "hostDocument",
        "hostDocument",
      ]);
      expect(
        attachments.map((a) =>
          a.source.case === "hostDocument" ? a.source.value.sessionId : a.source.case,
        ),
      ).to.deep.equal([
        ORCHESTRATOR_SESSION_ID,
        ORCHESTRATOR_SESSION_ID,
        ORCHESTRATOR_SESSION_ID,
        ORCHESTRATOR_SESSION_ID,
      ]);
    });
  });
});
