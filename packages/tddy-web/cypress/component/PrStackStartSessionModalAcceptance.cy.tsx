/**
 * Acceptance tests: the PR-Stack view's "Start session" CTA opens the shared session-creation
 * modal, pre-filled from the planned-PR node, instead of firing StartSession immediately. This
 * lets the operator review and adjust the branch, prompt, and parent before the child spawns.
 *
 * PRD: docs/ft/web/session-drawer.md § PR-Stack Chat Screen. The modal reuses `CreateSessionPane`
 * (the same form the sessions drawer uses), so it loads its own projects/agents/tools/sessions on
 * mount — the backend seeds those alongside the orchestrator session.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { ConnectionService, type ProjectEntry, type SessionEntry } from "../../src/gen/connection_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aStackPlanJson } from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-3333-0000-0000-0000-000000000030";
const PROJECT_ID = "proj-pr-stack";
const CHILD_SESSION_ID = "child-session-modal-1";

const NODE_TITLE = "Add token store";
const NODE_DESCRIPTION = "Persist and retrieve auth tokens";
const NODE_BRANCH = "feature/auth-stack/token-store";

const ORCHESTRATOR_SESSION: Partial<SessionEntry> = {
  sessionId: ORCHESTRATOR_SESSION_ID,
  createdAt: "2026-07-02T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  pid: 0,
  isActive: false,
  projectId: PROJECT_ID,
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "pr-stack",
  stackPlanJson: aStackPlanJson(1, [
    aPlannedNode({
      nodeId: "n1",
      title: NODE_TITLE,
      description: NODE_DESCRIPTION,
      branchSuggestion: NODE_BRANCH,
    }),
  ]),
};

const PROJECT: Partial<ProjectEntry> = {
  projectId: PROJECT_ID,
  name: "pr-stack-project",
  gitUrl: "https://example.com/pr-stack.git",
  mainRepoPath: "/home/dev/pr-stack-project",
  daemonInstanceId: "local",
};

/**
 * A backend seeded for the whole flow: the orchestrator session in the drawer, plus every RPC the
 * reused `CreateSessionPane` fetches on mount, plus the StartSession the dialog submits.
 */
function aPrStackModalBackend() {
  return aSessionsDrawerBackend([ORCHESTRATOR_SESSION])
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [{ id: "claude", label: "Claude" }] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }] }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({ branches: [] }))
    .onUnary(ConnectionService.method.startSession, () => ({
      sessionId: CHILD_SESSION_ID,
      livekitRoom: "room-child-modal-1",
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

it("opens the session-creation dialog when Start session is clicked, without starting a session yet", () => {
  // Given
  const backend = aPrStackModalBackend();

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("n1").click();

  // Then — the shared creation form appears in a dialog and no StartSession has fired.
  prStackScreenPage.createSessionDialog().should("be.visible");
  prStackScreenPage.createSessionPaneInDialog().should("be.visible");
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(ConnectionService.method.startSession)).to.have.length(0);
  });
});

it("pre-fills the planned PR's branch and prompt in the dialog", () => {
  // Given
  const backend = aPrStackModalBackend();

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("n1").click();

  // Then — the new-branch name is the planned branch and the prompt is the title + description.
  prStackScreenPage.dialogNewBranchNameInput().should("have.value", NODE_BRANCH);
  prStackScreenPage
    .dialogInitialPromptInput()
    .should("have.value", `${NODE_TITLE}\n\n${NODE_DESCRIPTION}`);
});

it("creates the stack-parented child session when the dialog is submitted", () => {
  // Given
  const backend = aPrStackModalBackend();

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("n1").click();
  prStackScreenPage.dialogSubmitBtn().click();

  // Then — StartSession is parented to this orchestrator, carries the planned branch as the new
  // branch name, and the child appears in the drawer.
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(1);
    expect(calls[0].stackParent).to.equal(ORCHESTRATOR_SESSION_ID);
    expect(calls[0].newBranchName).to.equal(NODE_BRANCH);
  });
  sessionsDrawerPage.drawerItem(CHILD_SESSION_ID).should("exist");
});
