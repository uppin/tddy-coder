/**
 * Acceptance tests: the PR-Stack Chat Screen must give the operator positive feedback, not just
 * surface failures. Two gaps observed in real use, even after the silent-failure fixes:
 *
 *   1. There was no persistent indication of whether the presenter room was connected at all —
 *      only a transient "connecting" overlay and an "error" banner exist; once a room actually
 *      reached "connected", nothing in the UI ever said so.
 *   2. Sending a message that genuinely succeeded gave no visible confirmation — the draft just
 *      cleared, and the operator's own text never appeared anywhere in the transcript (only
 *      inbound `AgentOutput` events render as bubbles). From the operator's point of view a
 *      successful send looked identical to typing into a `/dev/null` text box.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views (chat window).
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { PrStackChat } from "../../src/components/sessions/prstack/PrStackChat";
import { TddyRemote, type ClientMessage } from "../../src/gen/tddy/v1/remote_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PR_STACK_SESSION = {
  sessionId: "pr-stack-chat-feedback-0000-0000-0000-000000000001",
  createdAt: "2026-07-02T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  pid: 0,
  isActive: false,
  projectId: "proj-pr-stack-chat-feedback",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "pr-stack",
  stackPlanJson: "",
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
// 1. Persistent connection status
// ---------------------------------------------------------------------------

it("shows a Connected status once the presenter room has connected", () => {
  // Given / When — the presenter room's own connection lifecycle reports "connected"
  cy.mount(
    <PrStackChat
      session={PR_STACK_SESSION as any}
      room={null}
      livekitServerIdentity="server"
      roomStatus="connected"
    />,
  );

  // Then
  prStackScreenPage.chatStatus().should("exist").and("contain.text", "Connected");
});

it("shows a Not connected status before any connection attempt has been made", () => {
  // Given / When — no roomStatus supplied: the real default before PrStackScreen's own
  // presenter room has ever been attempted
  cy.mount(<PrStackChat session={PR_STACK_SESSION as any} room={null} livekitServerIdentity="server" />);

  // Then
  prStackScreenPage.chatStatus().should("exist").and("contain.text", "Not connected");
});

// ---------------------------------------------------------------------------
// 2. Sending a message must give visible confirmation, not just clear the draft
// ---------------------------------------------------------------------------

it("adds the operator's own sent message to the transcript immediately", () => {
  // Given — a presenter stream that accepts and records intents (a genuinely successful send)
  const sentIntents: ClientMessage[] = [];
  async function* recordingStream(requests: AsyncIterable<ClientMessage>) {
    for await (const req of requests) {
      sentIntents.push(req);
    }
  }
  const backend = aSessionsDrawerBackend([PR_STACK_SESSION]).implement(TddyRemote, {
    stream: recordingStream,
    getSession: async () => ({}),
    listSessions: async () => ({ sessions: [] }),
  });

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();
  prStackScreenPage.sendChatMessage("Split the auth feature into a PR stack.");

  // Then — the operator's own text shows up as a bubble, proving the send actually happened,
  // not just that the input was cleared
  prStackScreenPage
    .chatMessage(0)
    .should("exist")
    .and("contain.text", "Split the auth feature into a PR stack.");
});
