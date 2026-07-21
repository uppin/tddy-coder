/**
 * Acceptance test: a workflow that fails server-side must be visible in the PR-Stack Chat Screen.
 *
 * When the daemon's workflow fails (e.g. a resumed cursor session relaunched under the wrong CLI:
 * the coding agent exits non-zero with "No conversation found with session ID …"), it reports the
 * failure to the client as a `WorkflowComplete { ok: false, message }` event on the presenter
 * stream. Previously the chat only rendered `AgentOutput` events and dropped every other event,
 * including `WorkflowComplete` — so a failed workflow left the chat silently empty with no error.
 * The chat must surface a failed `WorkflowComplete` in its inline error banner.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views (chat window).
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { TddyRemote } from "../../src/gen/tddy/v1/remote_pb";
import { AcpService } from "../../src/gen/tddy/acp/v1/acp_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { acpError, acpScriptedSession } from "../support/rpc/acpSession";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PR_STACK_SESSION = {
  sessionId: "pr-stack-workflow-error-0000-0000-0000-000000000001",
  createdAt: "2026-07-02T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  pid: 0,
  isActive: false,
  projectId: "proj-pr-stack-workflow-error",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "pr-stack",
  stackPlanJson: "",
};

/**
 * A failed workflow: the daemon maps `WorkflowComplete { ok: false }` to an ACP `error` frame, which
 * the chat surfaces in its inline error banner (instead of ever producing an `AgentMessageChunk`).
 */
const aFailedWorkflowCompleteMessage = acpScriptedSession(
  acpError(
    "Claude Code CLI exited with code 1: No conversation found with session ID: pr-stack-workflow-error",
  ),
);

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

it("shows an inline error when the workflow fails instead of leaving the chat silently empty", () => {
  // Given — a presenter stream whose workflow fails (a failed WorkflowComplete, never an AgentOutput)
  const backend = aSessionsDrawerBackend([PR_STACK_SESSION])
    .implement(TddyRemote, {
      getSession: async () => ({}),
      listSessions: async () => ({ sessions: [] }),
    })
    .implement(AcpService, { session: aFailedWorkflowCompleteMessage });

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then — the failure is surfaced in the chat's inline error banner
  prStackScreenPage
    .chatError()
    .should("exist")
    .and("contain.text", "No conversation found with session ID");
});
