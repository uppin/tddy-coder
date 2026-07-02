/**
 * Acceptance tests: while the PR-Stack Chat Screen's own dedicated presenter LiveKit room (see
 * `usePresenterLiveKitRoom`) is still connecting — the token request and LiveKit handshake take
 * a few seconds after a session is opened — the chat panel must show that plainly instead of
 * looking ready to use. Previously an operator who typed and sent a message during that window
 * got "Message not sent — no connection to the presenter yet.", which reads like a random
 * failure rather than "you were just a little early."
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views (chat window).
 */

import React from "react";
import { PrStackChat } from "../../src/components/sessions/prstack/PrStackChat";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PR_STACK_SESSION = {
  sessionId: "pr-stack-connecting-overlay-0000-0000-000000000001",
  createdAt: "2026-07-02T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  pid: 0,
  isActive: false,
  projectId: "proj-pr-stack-connecting-overlay",
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
// Tests
// ---------------------------------------------------------------------------

it("shows a connecting overlay while the presenter room is still establishing its connection", () => {
  // Given — the presenter room's own connection lifecycle reports "connecting": the token
  // request or LiveKit handshake is still in flight (see usePresenterLiveKitRoom / useCommonRoom)
  cy.mount(
    <PrStackChat
      session={PR_STACK_SESSION as any}
      room={null}
      livekitServerIdentity="server"
      roomStatus="connecting"
    />,
  );

  // Then
  prStackScreenPage
    .chatConnectingOverlay()
    .should("exist")
    .and("contain.text", "Connecting");
});

it("disables the send button while the presenter room is still connecting", () => {
  // Given — same in-flight connection as above
  cy.mount(
    <PrStackChat
      session={PR_STACK_SESSION as any}
      room={null}
      livekitServerIdentity="server"
      roomStatus="connecting"
    />,
  );

  // Then — sending cannot be attempted at all, so the operator can never hit the
  // "no connection to the presenter yet" error just for being a little too fast
  prStackScreenPage.chatSendBtn().should("be.disabled");
});

it("does not show the connecting overlay once the presenter room has connected", () => {
  // Given — the connection lifecycle has settled into "connected"
  cy.mount(
    <PrStackChat
      session={PR_STACK_SESSION as any}
      room={null}
      livekitServerIdentity="server"
      roomStatus="connected"
    />,
  );

  // Then
  prStackScreenPage.chatConnectingOverlay().should("not.exist");
  prStackScreenPage.chatSendBtn().should("not.be.disabled");
});
