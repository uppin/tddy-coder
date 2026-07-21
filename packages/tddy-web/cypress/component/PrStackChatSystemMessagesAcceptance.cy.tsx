/**
 * Acceptance tests: the PR-Stack Chat Screen surfaces workflow progress — `GoalStarted` and
 * `ActivityLogged` presenter events — as distinct "system" bubbles in the message list, so the
 * transcript isn't silent while the agent works between spoken responses.
 *
 * Before this, `goalStarted`/`activityLogged` events were only used for internal mode tracking
 * and debug logs — never rendered at all (confirmed via live log trace: `recv #67 goalStarted`,
 * `recv #69 activityLogged` produced no visible change in the chat transcript).
 *
 * A `UserPrompt`-kind `activityLogged` event (the presenter's own "Queued: <text>" log line for
 * a `QueuePrompt` intent) must NOT become a second bubble — the operator's own message is already
 * echoed immediately by `sendPrompt`, so rendering the server's activity-log copy too would show
 * the same text twice.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views (chat window).
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { TddyRemote } from "../../src/gen/tddy/v1/remote_pb";
import { AcpService, type AcpAgentMessage, type AcpClientMessage } from "../../src/gen/tddy/acp/v1/acp_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { acpActivity, acpGoal, acpUserMessage } from "../support/rpc/acpSession";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PR_STACK_SESSION = {
  sessionId: "pr-stack-system-messages-0000-0000-0000-000000000001",
  createdAt: "2026-07-03T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  pid: 0,
  isActive: false,
  projectId: "proj-pr-stack-system-messages",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "pr-stack",
  stackPlanJson: "",
};

/** Yields `initialMessages` on stream open, then records every prompt/permission the client sends
 *  and reacts via `onPrompt` (the ACP counterpart of the old ServerMessage stream harness). */
function aScriptedSystemMessageBackend(
  initialMessages: AcpAgentMessage[],
  onPrompt?: () => AcpAgentMessage | undefined,
) {
  const sentIntents: AcpClientMessage[] = [];
  async function* session(requests: AsyncIterable<AcpClientMessage>) {
    for (const msg of initialMessages) {
      yield msg;
    }
    for await (const req of requests) {
      const c = req.msg.case;
      if (c !== "prompt" && c !== "requestPermission") continue;
      sentIntents.push(req);
      const follow = c === "prompt" ? onPrompt?.() : undefined;
      if (follow) yield follow;
    }
  }
  const backend = aSessionsDrawerBackend([PR_STACK_SESSION])
    .implement(TddyRemote, {
      getSession: async () => ({}),
      listSessions: async () => ({ sessions: [] }),
    })
    .implement(AcpService, { session });
  return { backend, sentIntents };
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

it("renders a goalStarted event as a goal bubble labeled with the goal name", () => {
  // Given
  const { backend } = aScriptedSystemMessageBackend([acpGoal("analyze-stack")]);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.chatMessage(0).should("exist").and("contain.text", "analyze-stack");
  prStackScreenPage.chatMessageKind(0).should("eq", "goal");
});

it("renders a non-UserPrompt activityLogged event as an activity bubble with its text", () => {
  // Given
  const { backend } = aScriptedSystemMessageBackend([
    acpActivity("Tool: grep pattern=\"analyze-stack\""),
  ]);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage
    .chatMessage(0)
    .should("exist")
    .and("contain.text", 'Tool: grep pattern="analyze-stack"');
  prStackScreenPage.chatMessageKind(0).should("eq", "activity");
});

it("does not render a duplicate bubble for a UserPrompt-kind activityLogged event already shown as the echoed user message", () => {
  // Given — the server echoes the operator's own queued prompt back as an ActivityLogged
  // entry with kind=UserPrompt (matches the real presenter's `QueuePrompt` handler), after the
  // operator's own message has already been echoed client-side by `sendPrompt`
  const { backend } = aScriptedSystemMessageBackend([], () =>
    // The agent echoes the operator's queued prompt back as a user_message_chunk; useAcpSession
    // ignores it (already echoed locally), so it must not produce a second bubble.
    acpUserMessage("Queued: Split this into three PRs."),
  );
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // When
  prStackScreenPage.sendChatMessage("Split this into three PRs.");

  // Then — exactly one bubble (the operator's own echoed message), no second "Queued: ..." bubble
  prStackScreenPage
    .chatMessage(0)
    .should("exist")
    .and("contain.text", "Split this into three PRs.");
  prStackScreenPage.chatMessage(1).should("not.exist");
});
