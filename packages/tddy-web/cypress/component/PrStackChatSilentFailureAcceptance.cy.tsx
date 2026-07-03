/**
 * Acceptance tests: the PR-Stack Chat Screen must never fail silently. Four distinct failure
 * modes were previously invisible to the operator:
 *
 *   1. The screen's own dedicated LiveKit room (see `usePresenterLiveKitRoom`) fails to connect
 *      (e.g. the token service is unavailable) — `useCommonRoom`'s `error` was discarded at the
 *      `usePresenterLiveKitRoom` boundary and never reached the chat panel.
 *   2. The operator sends a message while there is no live client to send it over — `sendPrompt`
 *      was a silent no-op and `PrStackChat.handleSend` unconditionally cleared the draft anyway,
 *      identical to a successful send.
 *   3. The presenter stream fails *after* a client was built — the read loop's catch block only
 *      `console.debug`d the failure; the chat just stopped updating with no operator-visible
 *      signal.
 *   4. The daemon-side presenter's own LiveKit participant is no longer in the room (e.g. it
 *      dropped off during a LiveKit server restart and never reconnected) while the browser's
 *      own presenter room connection is otherwise healthy. `LiveKitTransport.publishData` with
 *      `destinationIdentities` is a fire-and-forget reliable data-channel send — LiveKit gives
 *      the sender no delivery/presence signal when the recipient is absent, so the message
 *      vanishes with no error and the response side of the stream just hangs forever. The fix
 *      must check `room.remoteParticipants` for the target identity before treating a send as
 *      accepted.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views (chat window).
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { ConnectError, Code } from "@connectrpc/connect";
import { Room } from "livekit-client";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { PrStackChat } from "../../src/components/sessions/prstack/PrStackChat";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import {
  TddyRemote,
  ServerMessageSchema,
  type ClientMessage,
} from "../../src/gen/tddy/v1/remote_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

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

const LIVEKIT_ATTACHMENT: SessionAttachmentState = {
  status: "connected-livekit",
  sessionId: PR_STACK_SESSION.sessionId,
  livekitRoom: "daemon-pr-stack-silent-failure-0001",
  livekitUrl: "wss://livekit.internal:7880",
  livekitServerIdentity: "daemon-pr-stack-silent-failure-0001",
  identity: "browser-pr-stack-silent-failure-0001-1719999999999",
};

/**
 * Yields a single `AgentOutput`, then throws — simulates the presenter stream dying mid-flight.
 * A `ConnectError` is required here (not a plain `Error`): the Connect protocol deliberately
 * replaces any plain exception thrown from a streaming handler with a generic "[internal]
 * internal error" before it reaches the client, so only a `ConnectError`'s message crosses
 * the wire intact.
 */
async function* streamThatDropsAfterOneMessage() {
  yield create(ServerMessageSchema, {
    event: { case: "agentOutput", value: { text: "Analyzing the feature into a PR stack…" } },
  });
  throw new ConnectError("presenter disconnected unexpectedly", Code.Unknown);
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

function PrStackMainPaneHarness() {
  return (
    <SessionMainPane
      selectedSession={PR_STACK_SESSION as any}
      attachment={LIVEKIT_ATTACHMENT}
      inspectorState="closed"
      onToggleInspector={cy.stub()}
      onInspectorClose={cy.stub()}
      onInspectorExpand={cy.stub()}
      onInspectorRestore={cy.stub()}
      onResume={cy.stub()}
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
// 1. Room connection failure must surface in the chat panel
// ---------------------------------------------------------------------------

it("shows an inline error in the chat panel when handed a failed room connection", () => {
  // Given — PrStackChat is told its presenter room connection failed, exactly as
  // PrStackScreen would report it after usePresenterLiveKitRoom surfaces useCommonRoom's error
  cy.mount(
    <PrStackChat
      session={PR_STACK_SESSION as any}
      room={null}
      livekitServerIdentity="server"
      roomStatus="error"
      roomError="livekit token service unavailable"
    />,
  );

  // Then
  prStackScreenPage
    .chatError()
    .should("exist")
    .and("contain.text", "livekit token service unavailable");
});

it("requests a browser LiveKit token for the presenter room and surfaces the failure when it cannot be issued", () => {
  // Given — the session's attachment is connected-livekit, but the token service that
  // usePresenterLiveKitRoom's useCommonRoom depends on is unavailable
  interceptGenerateTokenUnavailable();

  // When
  cy.mount(<PrStackMainPaneHarness />);
  cy.wait("@generateTokenUnavailable");

  // Then — the failure travels: useCommonRoom -> usePresenterLiveKitRoom -> PrStackScreen ->
  // PrStackChat, instead of being discarded at any of those boundaries
  prStackScreenPage
    .chatError()
    .should("exist")
    .and("contain.text", "livekit token service unavailable");
});

// ---------------------------------------------------------------------------
// 2. Sending with no live client must not silently succeed
// ---------------------------------------------------------------------------

it("keeps the typed draft and shows an error instead of clearing the input when sending with no presenter connection", () => {
  // Given — no LiveKit room and no test-double transport override: the real production
  // state before a room ever connects (canBuildClient is false, so sendPrompt has nothing
  // to enqueue onto)
  cy.mount(<PrStackChat session={PR_STACK_SESSION as any} room={null} livekitServerIdentity="server" />);

  // When
  prStackScreenPage.sendChatMessage("Split the auth feature into a PR stack.");

  // Then — the message text is preserved (not silently discarded) and an error is shown
  prStackScreenPage.chatInput().should("have.value", "Split the auth feature into a PR stack.");
  prStackScreenPage
    .chatError()
    .should("exist")
    .and("contain.text", "no connection to the presenter");
});

// ---------------------------------------------------------------------------
// 3. A stream failure after connecting must surface, not just stop the chat silently
// ---------------------------------------------------------------------------

it("shows an inline error when the presenter stream fails after the workflow was already talking", () => {
  // Given — a presenter stream that delivers one message, then dies
  const backend = aSessionsDrawerBackend([PR_STACK_SESSION]).implement(TddyRemote, {
    stream: streamThatDropsAfterOneMessage,
    getSession: async () => ({}),
    listSessions: async () => ({ sessions: [] }),
  });

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();
  prStackScreenPage.chatMessage(0).should("exist");

  // Then
  prStackScreenPage
    .chatError()
    .should("exist")
    .and("contain.text", "presenter disconnected unexpectedly");
});

// ---------------------------------------------------------------------------
// 4. Sending to a presenter identity that's no longer in the room must not vanish silently
// ---------------------------------------------------------------------------

it("shows an error and preserves the draft when the presenter's own participant is no longer in the room", () => {
  // Given — a real, unconnected LiveKit Room: its remoteParticipants map is empty, exactly the
  // state observed when the daemon-side tddy-coder process has dropped off the room (e.g. after
  // a LiveKit server restart) while the browser's own presenter connection is otherwise healthy.
  // A stream is registered so a client can build, but no intent should ever reach it — the
  // presence check must block the send before anything is transmitted.
  const sentIntents: ClientMessage[] = [];
  async function* recordingStream(requests: AsyncIterable<ClientMessage>) {
    for await (const req of requests) {
      // Ignore the eager stream-open frame (an intent-less ClientMessage the hook enqueues to open
      // the stream); only the operator's actual intents are under test.
      if (req.intent.case === undefined) continue;
      sentIntents.push(req);
    }
  }
  const backend = aSessionsDrawerBackend([PR_STACK_SESSION]).implement(TddyRemote, {
    stream: recordingStream,
    getSession: async () => ({}),
    listSessions: async () => ({ sessions: [] }),
  });

  // When
  mountWithRpc(
    <PrStackChat
      session={PR_STACK_SESSION as any}
      room={new Room()}
      livekitServerIdentity="daemon-dev-presenter-not-in-room"
    />,
    backend,
  );
  prStackScreenPage.sendChatMessage("hi");

  // Then — the draft is preserved, an error appears, and nothing was ever transmitted
  prStackScreenPage.chatInput().should("have.value", "hi");
  prStackScreenPage
    .chatError()
    .should("exist")
    .and("contain.text", "presenter is not connected");
  cy.wrap(sentIntents).should("have.length", 0);
});
