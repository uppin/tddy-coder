/**
 * Acceptance tests: the PR-Stack Chat Screen's chat window implements a remote Presenter over
 * the existing bidirectional `TddyRemote.Stream` RPC — inbound `PresenterEvent`s render as chat
 * bubbles, and user input is sent as `UserIntent`s (`ClientMessage` oneof).
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views (chat window).
 * Changeset: docs/dev/1-WIP/pr-stack-workflow-views.md.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { PrStackChat } from "../../src/components/sessions/prstack/PrStackChat";
import { TddyRemote, ServerMessageSchema, type ClientMessage } from "../../src/gen/tddy/v1/remote_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PR_STACK_SESSION = {
  sessionId: "pr-stack-session-3333-0000-0000-0000-000000000030",
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
  stackPlanJson: "",
};

/** Yields a single `AgentOutput` ServerMessage, then completes — the request stream is ignored. */
async function* oneAgentOutputMessage() {
  yield create(ServerMessageSchema, {
    event: { case: "agentOutput", value: { text: "Analyzing the feature into a PR stack…" } },
  });
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

it("renders an agent output event from the presenter stream as a chat bubble", () => {
  // Given
  const chatBackend = aSessionsDrawerBackend([PR_STACK_SESSION]).implement(TddyRemote, {
    stream: oneAgentOutputMessage,
    getSession: async () => ({}),
    listSessions: async () => ({ sessions: [] }),
  });

  // When
  mountWithRpc(<SessionsDrawerScreen />, chatBackend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.chat().should("exist");
  prStackScreenPage
    .chatMessage(0)
    .should("exist")
    .and("contain.text", "Analyzing the feature into a PR stack…");
});

function aRecordingChatBackend() {
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
  return { backend, sentIntents };
}

it("sends a SubmitFeatureInput intent for the first message on a fresh session", () => {
  // Given — a fresh session: the presenter has never broadcast a ModeChanged event, so the
  // workflow has not started yet (matches every recipe's actual FeatureInput starting mode).
  const { backend, sentIntents } = aRecordingChatBackend();

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();
  prStackScreenPage.sendChatMessage("Split the auth feature into a PR stack.");

  // Then — assert the case and the text field individually; the value is a live
  // `@bufbuild/protobuf` v2 message instance and also carries its own `$typeName`.
  cy.wrap(sentIntents).should((intents) => {
    expect(intents).to.have.length(1);
    expect(intents[0].intent.case).to.equal("submitFeatureInput");
    expect(intents[0].intent.value.text).to.equal("Split the auth feature into a PR stack.");
  });
});

it("sends a QueuePrompt intent for a message sent after the workflow has already started", () => {
  // Given — the first message already started the workflow (SubmitFeatureInput)
  const { backend, sentIntents } = aRecordingChatBackend();
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();
  prStackScreenPage.sendChatMessage("Split the auth feature into a PR stack.");

  // When — the operator sends a follow-up nudge while the workflow is running
  prStackScreenPage.sendChatMessage("Split this into three PRs instead of two.");

  // Then
  cy.wrap(sentIntents).should((intents) => {
    expect(intents).to.have.length(2);
    expect(intents[1].intent.case).to.equal("queuePrompt");
    expect(intents[1].intent.value.text).to.equal(
      "Split this into three PRs instead of two.",
    );
  });
});

it("renders the chat panel when no LiveKit room has been attached to the session yet", () => {
  // Given — a pr-stack session before any LiveKit room is connected: the real production
  // state today, since nothing yet threads a connected orchestrator Room through to this
  // screen (see the TODO in usePresenterChat.ts). No RpcTransportProvider override here —
  // this deliberately exercises the real default LiveKit transport factory, not the
  // in-memory fake the other tests in this file use, to catch a factory that assumes a
  // non-null room.

  // When
  cy.mount(<PrStackChat session={PR_STACK_SESSION} room={null} livekitServerIdentity="server" />);

  // Then
  prStackScreenPage.chat().should("exist");
});
