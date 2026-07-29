/**
 * Acceptance tests: the PR-Stack "Start session" CTA opens the shared dialog whose new-branch option
 * names the concrete base branch it will branch from — the planned node's predecessor stack branch
 * (e.g. "New branch from base: feature/auth/token-store") rather than a static label. A root node
 * (no predecessor) branches from the project's default branch.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-07-25-branch-query-and-remote-branch.md.
 * Changeset: docs/dev/1-WIP/2026-07-25-branch-query-and-remote-branch.md.
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

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-5555-0000-0000-0000-000000000050";
const PROJECT_ID = "proj-pr-stack";
const PREDECESSOR_BRANCH = "feature/auth/token-store";

const ORCHESTRATOR_SESSION: Partial<SessionEntry> = {
  sessionId: ORCHESTRATOR_SESSION_ID,
  createdAt: "2026-07-06T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  isActive: false,
  projectId: PROJECT_ID,
  recipe: "pr-stack",
  // n1 (predecessor, spawned) → n2 depends on n1 and is still unspawned.
  stackPlanJson: aStackPlanJson(1, [
    aPlannedNode({
      nodeId: "n1",
      title: "Add token store",
      branch: PREDECESSOR_BRANCH,
      sessionId: "child-n1",
      prStatus: { phase: "open" },
    }),
    aPlannedNode({
      nodeId: "n2",
      title: "Add middleware",
      branchSuggestion: "feature/auth/middleware",
      parents: ["n1"],
    }),
  ]),
};

const PROJECT: Partial<ProjectEntry> = {
  projectId: PROJECT_ID,
  name: "pr-stack-project",
  gitUrl: "https://example.com/pr-stack.git",
  mainRepoPath: "/home/dev/pr-stack-project",
  mainBranchRef: "origin/master",
  daemonInstanceId: "local",
};

function aPrStackModalBackend() {
  return aSessionsDrawerBackend([ORCHESTRATOR_SESSION])
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [{ id: "claude", label: "Claude" }] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }] }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({ branches: [], defaultRemote: "origin" }));
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

it("names the predecessor stack branch as the base in the dialog's new-branch option", () => {
  // Given
  const backend = aPrStackModalBackend();

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("n2").click();

  // Then — the base branch shown is n2's predecessor (n1's branch), not a static label. The label
  // is lifted to the project's remote-tracking ref (`<remote>/<branch>`, here `origin/...`) so it
  // matches the base-branch picker's options and reads the ref the daemon will fetch — see
  // PrStackScreen.tsx (remoteTrackingName over deriveStackBaseBranch).
  prStackScreenPage
    .dialogBranchIntentSelect()
    .should("contain.text", `New branch from base: origin/${PREDECESSOR_BRANCH}`);
});
