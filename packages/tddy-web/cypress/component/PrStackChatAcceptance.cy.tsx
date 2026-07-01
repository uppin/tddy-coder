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

it("sends a QueuePrompt intent on the presenter stream when the operator submits a chat message", () => {
  // Given
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
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();
  prStackScreenPage.sendChatMessage("Split this into three PRs instead of two.");

  // Then — assert the case and the text field individually; the value is a live
  // `@bufbuild/protobuf` v2 message instance and also carries its own `$typeName`.
  cy.wrap(sentIntents).should((intents) => {
    expect(intents).to.have.length(1);
    expect(intents[0].intent.case).to.equal("queuePrompt");
    expect(intents[0].intent.value.text).to.equal(
      "Split this into three PRs instead of two.",
    );
  });
});
