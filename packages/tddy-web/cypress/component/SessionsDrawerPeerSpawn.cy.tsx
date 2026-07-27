/**
 * Acceptance tests: the "Add agent" button in the session-detail header spawns a peer child session
 * sharing the current session's workspace, reusing the existing `stack_parent` / orchestrator
 * spawn path. The peer appears in the drawer via the optimistic overlay, and the StartSession
 * request carries `stackParent` = the current session's id (no proto/daemon changes).
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-07-27-session-agent.md
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { ConnectionService, type ProjectEntry, type SessionEntry } from "../../src/gen/connection_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionAgentsPage } from "../support/pages/sessionAgentsPage";
import { TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const CURRENT_SESSION_ID = "session-current-aaaa-0000-0000-000000000001";
const PROJECT_ID = "proj-session-agents";
const PEER_SESSION_ID = "peer-cursor-aaaa-0000-0000-000000000002";

const CURRENT_SESSION: Partial<SessionEntry> = {
  sessionId: CURRENT_SESSION_ID,
  createdAt: "2026-07-27T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/session-agents-project",
  pid: 11111,
  isActive: true,
  projectId: PROJECT_ID,
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  agent: "claude",
  model: "sonnet-4",
};

const PROJECT: Partial<ProjectEntry> = {
  projectId: PROJECT_ID,
  name: "session-agents-project",
  gitUrl: "https://example.com/session-agents.git",
  mainRepoPath: "/home/dev/session-agents-project",
  daemonInstanceId: "local",
};

/**
 * A backend seeded for the whole flow: the current session in the drawer, plus every RPC the
 * reused `CreateSessionPane` fetches on mount, plus the StartSession the dialog submits.
 */
function aSessionAgentsBackend() {
  return aSessionsDrawerBackend([CURRENT_SESSION])
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [{ id: "claude", label: "Claude" }] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }] }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({ branches: [] }))
    .onUnary(ConnectionService.method.startSession, () => ({
      sessionId: PEER_SESSION_ID,
      livekitRoom: "room-peer-cursor-1",
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

it("renders the Add agent button when a session is selected in the drawer", () => {
  // Given
  const backend = aSessionAgentsBackend();

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(CURRENT_SESSION_ID).click();

  // Then
  sessionAgentsPage.addAgentBtn().should("be.visible");
});

it("opens the session-creation pane when Add agent is clicked, without starting a session yet", () => {
  // Given
  const backend = aSessionAgentsBackend();

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(CURRENT_SESSION_ID).click();
  sessionAgentsPage.addAgentBtn().click();

  // Then — the shared creation form appears and no StartSession has fired.
  sessionsDrawerPage.createSessionPane().should("be.visible");
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(ConnectionService.method.startSession)).to.have.length(0);
  });
});

it("creates a peer session on the same worktree when the creation pane is submitted", () => {
  // Given
  const backend = aSessionAgentsBackend();

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(CURRENT_SESSION_ID).click();
  sessionAgentsPage.addAgentBtn().click();
  sessionsDrawerPage.createSessionSubmitBtn().click();

  // Then — StartSession is parented to the current session AND reuses its worktree via repo_path
  // (no new git worktree, no branch checkout). Branch fields are absent/empty.
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(1);
    expect(calls[0].stackParent).to.equal(CURRENT_SESSION_ID);
    expect(calls[0].projectId).to.equal(PROJECT_ID);
    expect(calls[0].repoPath).to.equal(CURRENT_SESSION.repoPath);
    expect(calls[0].newBranchName).to.equal("");
    expect(calls[0].selectedBranchToWorkOn).to.equal("");
  });
  sessionsDrawerPage.drawerItem(PEER_SESSION_ID).should("exist");
});

it("hides branch selection controls in the peer spawn dialog", () => {
  // Given
  const backend = aSessionAgentsBackend();

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(CURRENT_SESSION_ID).click();
  sessionAgentsPage.addAgentBtn().click();

  // Then — the branch intent selector, new-branch name input, and branch-to-work-on selector
  // are all absent in peer mode (irrelevant when repo_path is set).
  cy.get(`[data-testid="${TEST_IDS.createSessionBranchIntentSelect}"]`).should("not.exist");
  cy.get(`[data-testid="${TEST_IDS.createSessionNewBranchNameInput}"]`).should("not.exist");
  cy.get(`[data-testid="${TEST_IDS.createSessionBranchToWorkOnSelect}"]`).should("not.exist");
});
