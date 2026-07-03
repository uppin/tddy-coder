/**
 * Acceptance tests: the PR-Stack Chat Screen's "Start session" CTA spawns a correctly-parented
 * child session for an unspawned planned PR.
 *
 * PRD: docs/ft/coder/pr-stacking.md § Child linking, docs/ft/web/session-drawer.md
 * § PR-Stack Chat Screen. Changeset: docs/dev/1-WIP/pr-stack-workflow-views.md.
 *
 * `StartSession.recipe` / `.stackParent` are existing proto fields (added for the parent
 * picker in #246) — no new RPC surface is needed for the CTA itself.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { ConnectionService } from "../../src/gen/connection_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aStackPlanJson } from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-2222-0000-0000-0000-000000000020";

const ORCHESTRATOR_SESSION = {
  sessionId: ORCHESTRATOR_SESSION_ID,
  createdAt: "2026-07-01T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  pid: 0,
  isActive: false,
  projectId: "proj-pr-stack",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "pr-stack",
  stackPlanJson: aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store" }),
  ]),
};

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

it("starts a stack-parented Claude CLI child session when Start session is clicked", () => {
  // Given
  const backend = aSessionsDrawerBackend([ORCHESTRATOR_SESSION]).onUnary(
    ConnectionService.method.startSession,
    () => ({
      sessionId: "child-session-new-1",
      livekitRoom: "room-child-1",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "daemon",
    }),
  );

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("n1").click();

  // Then — planned-PR sessions default to a Claude CLI session (see PrStackScreen), stack-parented
  // to this orchestrator so the child worktree chains onto its branch.
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(1);
    expect(calls[0].sessionType).to.equal("claude-cli");
    expect(calls[0].stackParent).to.equal(ORCHESTRATOR_SESSION_ID);
  });
});

it("starts a planned PR with its planned branch as new_branch_name", () => {
  // Given — the planned node carries a grouped stack branch (feature/<stack>/<node>), pre-filled
  // by the pr-stack agent.
  const session = {
    ...ORCHESTRATOR_SESSION,
    stackPlanJson: aStackPlanJson(1, [
      aPlannedNode({
        nodeId: "n1",
        title: "Add token store",
        branchSuggestion: "feature/auth-stack/token-store",
      }),
    ]),
  };
  const backend = aSessionsDrawerBackend([session]).onUnary(
    ConnectionService.method.startSession,
    () => ({ sessionId: "child-session-branch-1" }),
  );

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("n1").click();

  // Then — the StartSession request carries the planned branch, not an empty name. The daemon
  // rejects branch_worktree_intent = new_branch_from_base with an empty new_branch_name.
  cy.wrap(null).should(() => {
    const calls = backend.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(1);
    expect(calls[0].newBranchName).to.eq("feature/auth-stack/token-store");
  });
});

it("navigates to the newly spawned child session after Start session succeeds", () => {
  // Given
  const backend = aSessionsDrawerBackend([ORCHESTRATOR_SESSION]).onUnary(
    ConnectionService.method.startSession,
    () => ({
      sessionId: "child-session-new-2",
      livekitRoom: "room-child-2",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "daemon",
    }),
  );

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.startSessionBtn("n1").click();

  // Then
  sessionsDrawerPage.drawerItem("child-session-new-2").should("exist");
});
