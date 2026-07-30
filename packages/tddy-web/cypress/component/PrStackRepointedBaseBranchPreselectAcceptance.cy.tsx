/**
 * Acceptance tests: a planned PR that was **repointed onto the project default branch** opens its
 * Start-session dialog with that branch already selected as the base.
 *
 * Live scenario (session 019f9dd5): "Create Session attach UI" was repointed onto master — the daemon
 * dropped its `attach-start` parent edge, and the dialog's "New branch from base:" label duly read
 * `origin/master`. The "Base branch" selector right below it, however, listed only the stack's own
 * branches and pre-selected the first of them, so starting the session would have based the child onto
 * `feature/session-attach-docs/attach-proto` and silently undone the repoint. The default branch was
 * not even offered as an option.
 *
 * The pre-selection must be the node's derived base (what the label promises), and the default branch
 * must be selectable.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Repointing a dead-end planned PR.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { ConnectionService, type ProjectEntry, type SessionEntry } from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aStackPlanJson } from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-repointbase-0000-0000-000000000080";
const PROJECT_ID = "proj-pr-stack";
const CHILD_SESSION_ID = "child-session-repointbase-1";

const REMOTE = "origin";
const DEFAULT_BRANCH_REF = `${REMOTE}/master`;
const ATTACH_PROTO_BRANCH = "feature/session-attach-docs/attach-proto";
const ATTACH_STORE_BRANCH = "feature/session-attach-docs/attach-store";
const ATTACH_PROTO_REF = `${REMOTE}/${ATTACH_PROTO_BRANCH}`;
const ATTACH_STORE_REF = `${REMOTE}/${ATTACH_STORE_BRANCH}`;

/** The project as the daemon reports it: `main_branch_ref` is a remote-tracking ref. */
const PROJECT: Partial<ProjectEntry> = {
  projectId: PROJECT_ID,
  name: "pr-stack-project",
  gitUrl: "https://example.com/pr-stack.git",
  mainRepoPath: "/home/dev/pr-stack-project",
  mainBranchRef: DEFAULT_BRANCH_REF,
  defaultRemote: REMOTE,
  daemonInstanceId: "local",
};

/**
 * The live stack after the repoint: `attach-ui` has no parents left, `attach-proto` and `attach-store`
 * are open PRs owning branches, and `attach-start` is still planned (branchless).
 */
function anAttachDocsOrchestratorWithRepointedUi(): Partial<SessionEntry> {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-07-30T04:02:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    isActive: false,
    projectId: PROJECT_ID,
    recipe: "pr-stack",
    stackPlanJson: aStackPlanJson(1, [
      aPlannedNode({
        nodeId: "attach-proto",
        title: "Start-session attachment proto",
        branch: ATTACH_PROTO_BRANCH,
        sessionId: "child-attach-proto",
        prStatus: { phase: "open" },
      }),
      aPlannedNode({
        nodeId: "attach-store",
        title: "Session attachment storage and context docs",
        branch: ATTACH_STORE_BRANCH,
        sessionId: "child-attach-store",
        prStatus: { phase: "open" },
      }),
      aPlannedNode({
        nodeId: "attach-start",
        title: "Copy attachments during StartSession",
        branchSuggestion: "feature/session-attach-docs/attach-start",
        parents: ["attach-proto", "attach-store"],
      }),
      // Repointed onto master: `RepointPlannedPr` dropped the `attach-start` parent edge.
      aPlannedNode({
        nodeId: "attach-ui",
        title: "Create Session attach UI",
        branchSuggestion: "feature/session-attach-docs/attach-ui",
        parents: [],
      }),
    ]),
  };
}

/**
 * A backend seeded for the whole flow: the orchestrator session in the drawer, every RPC the reused
 * `CreateSessionPane` fetches on mount, and the StartSession the dialog submits.
 */
function aRepointedBaseBackend(orchestrator: Partial<SessionEntry>) {
  return aSessionsDrawerBackend([orchestrator])
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [{ id: "claude", label: "Claude" }] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }] }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({ branches: [], defaultRemote: REMOTE }))
    .onUnary(ConnectionService.method.startSession, () => ({
      sessionId: CHILD_SESSION_ID,
      livekitRoom: "room-child-repointbase-1",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "daemon",
    }));
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("pre-selects the project default branch for a planned PR repointed onto it", () => {
  // Given — "Create Session attach UI", repointed onto master (no parents left).
  const backend = aRepointedBaseBackend(anAttachDocsOrchestratorWithRepointedUi());

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("attach-ui").click();

  // Then — master is selected, not the stack branch that happens to be listed first.
  prStackScreenPage.dialogBaseBranchSelect().should("have.value", DEFAULT_BRANCH_REF);
});

it("offers the project default branch alongside the stack branches for a repointed planned PR", () => {
  // Given
  const backend = aRepointedBaseBackend(anAttachDocsOrchestratorWithRepointedUi());

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("attach-ui").click();

  // Then — the stack's materialized branches, then the project default: an operator who repointed onto
  // master can see and re-pick it.
  prStackScreenPage
    .dialogBaseBranchOptionValues()
    .should("deep.equal", [ATTACH_PROTO_REF, ATTACH_STORE_REF, DEFAULT_BRANCH_REF]);
});

it("sends the project default branch as selected_integration_base_ref for a repointed planned PR", () => {
  // Given
  const backend = aRepointedBaseBackend(anAttachDocsOrchestratorWithRepointedUi());

  // When — the operator accepts the dialog as pre-filled, touching nothing.
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("attach-ui").click();
  prStackScreenPage.dialogSubmitBtn().click();

  // Then — the child is based onto master, matching the repoint and the dialog's own base label.
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(1);
    expect(calls[0].stackParent).to.equal(ORCHESTRATOR_SESSION_ID);
    expect(calls[0].selectedIntegrationBaseRef).to.equal(DEFAULT_BRANCH_REF);
  });
});
