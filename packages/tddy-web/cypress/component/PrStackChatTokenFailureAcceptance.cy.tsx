/**
 * Acceptance: a failed presenter-room token request reaches the PR-Stack chat panel.
 *
 * Stays in tddy-web: the failure travels through the app's real LiveKit room join
 * (`useCommonRoom` -> `usePresenterRoom`), which mints a browser token over the app's HTTP transport
 * (`cy.intercept` on `token.TokenService/GenerateToken`) — neither exists behind the package's port
 * doubles.
 *
 * Moved out of `tddy-session-web`'s `PrStackChatSilentFailureAcceptance.cy.tsx` (#web-split 18/20).
 */

import React from "react";
import { SessionMainPane } from "tddy-session-web";
import type { SessionAttachmentState } from "tddy-session-web";
import { aSessionConnection } from "../support/rpc/sessionConnections";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

const PR_STACK_SESSION = {
  sessionId: "pr-stack-silent-failure-0000-0000-0000-000000000001",
  createdAt: "2026-07-02T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  pid: 0,
  isActive: false,
  projectId: "proj-pr-stack-silent-failure",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "pr-stack",
  stackPlanJson: "",
};

const ATTACHED_OVER_A_ROOM = aSessionConnection(PR_STACK_SESSION.sessionId).carriedByRoom(
  "daemon-pr-stack-silent-failure-0001",
  { url: "wss://livekit.internal:7880", serverIdentity: "daemon-pr-stack-silent-failure-0001" },
);
const LIVEKIT_ATTACHMENT: SessionAttachmentState = {
  status: "connected",
  connection: ATTACHED_OVER_A_ROOM.build(),
};
const LIVEKIT_HINT = ATTACHED_OVER_A_ROOM.buildHint();

function PrStackMainPaneHarness() {
  return (
    <SessionMainPane
      host={null}
      selectedSession={PR_STACK_SESSION as any}
      attachment={LIVEKIT_ATTACHMENT}
      attachmentHint={LIVEKIT_HINT}
      inspectorState="closed"
      onToggleInspector={cy.stub()}
      onInspectorClose={cy.stub()}
      onInspectorExpand={cy.stub()}
      onInspectorRestore={cy.stub()}
      onResume={cy.stub()}
      onResumeSession={cy.stub()}
      onDelete={cy.stub()}
      onTerminate={cy.stub()}
    />
  );
}

function interceptGenerateTokenUnavailable() {
  cy.intercept("POST", "**/rpc/token.TokenService/GenerateToken", (req) => {
    req.reply({
      statusCode: 503,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code: "unavailable", message: "livekit token service unavailable" }),
    });
  }).as("generateTokenUnavailable");
}

beforeEach(() => {
  cy.viewport(1280, 800);
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

it("requests a browser LiveKit token for the presenter room and surfaces the failure when it cannot be issued", () => {
  // Given — the session is attached over its own room, but the token service that
  // usePresenterRoom's useCommonRoom depends on is unavailable
  interceptGenerateTokenUnavailable();

  // When
  cy.mount(<PrStackMainPaneHarness />);
  cy.wait("@generateTokenUnavailable");

  // Then — the failure travels: useCommonRoom -> usePresenterRoom -> PrStackScreen ->
  // PrStackChat, instead of being discarded at any of those boundaries
  prStackScreenPage
    .chatError()
    .should("exist")
    .and("contain.text", "livekit token service unavailable");
});

