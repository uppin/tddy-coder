/**
 * Acceptance test: the Inspector drawer must be available regardless of which main-pane view is
 * showing. A custom per-workflow view (e.g. the PR-Stack Chat Screen) only replaces the terminal
 * — it must not also swallow the Inspector overlay.
 *
 * Previously `SessionMainPane` only rendered `<SessionInspectorDrawer>` inside its
 * terminal-connected and disconnected-placeholder branches; the `customView` branch rendered
 * nothing else at all. The "Inspector" toggle button (gated only on `selectedSession`, not on
 * which branch is active) still appeared and still updated `inspectorState` when clicked, but
 * there was no drawer element in the DOM for a pr-stack session to ever reflect that state —
 * the toggle looked functional but did nothing.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views (chat window).
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { TddyRemote } from "../../src/gen/tddy/v1/remote_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PR_STACK_SESSION = {
  sessionId: "pr-stack-inspector-0000-0000-0000-000000000001",
  createdAt: "2026-07-02T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  pid: 0,
  isActive: false,
  projectId: "proj-pr-stack-inspector",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "pr-stack",
  stackPlanJson: "",
};

/** Empty presenter stream — no messages needed for this test. */
async function* emptyStream() {}

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
// Test
// ---------------------------------------------------------------------------

it("opens the inspector as an overlay for a pr-stack session, leaving the PR-Stack Chat Screen visible", () => {
  // Given
  const backend = aSessionsDrawerBackend([PR_STACK_SESSION]).implement(TddyRemote, {
    stream: emptyStream,
    getSession: async () => ({}),
    listSessions: async () => ({ sessions: [] }),
  });

  // When — PR_STACK_SESSION is disconnected (isActive: false), which auto-opens the inspector
  // on selection (see SessionInspectorAcceptance AC3) — no toggle click needed
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then — the drawer is present and open, and the pr-stack screen (the "terminal replacement")
  // stays mounted underneath it, exactly like the terminal does for regular sessions
  sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");
  prStackScreenPage.screen().should("exist");
});
