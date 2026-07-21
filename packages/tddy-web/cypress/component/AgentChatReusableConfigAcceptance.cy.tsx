/**
 * Acceptance: `AgentChat` is recipe-agnostic and reusable. It takes a plain `placeholder` prop
 * (not a `SessionEntry`), exposes `agent-chat-*` test ids, and — mounted with nothing but a
 * transport — lets the operator send a first prompt as a `SubmitFeatureInput` intent.
 *
 * This pins the extraction contract: the component must NOT depend on `SessionEntry`, PR-stack
 * types, or the sessions drawer. The old `PrStackChat` derived its input placeholder from
 * `session.sessionId.slice(0, 8)`; the reusable component takes it as a prop.
 *
 * PRD: docs/ft/web/session-drawer.md § Agent Chat.
 */

import React from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AgentChat } from "../../src/components/chat/AgentChat";
import { TddyRemote, type ClientMessage } from "../../src/gen/tddy/v1/remote_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentChatPage } from "../support/pages/agentChatPage";

const PLACEHOLDER = "Ask the coding agent anything…";

/** A backend that opens the stream (so the input is live) and records every intent sent. */
function aRecordingBackend() {
  const sentIntents: ClientMessage[] = [];
  async function* stream(requests: AsyncIterable<ClientMessage>) {
    const it = requests[Symbol.asyncIterator]();
    await it.next(); // eager empty open frame — no intent, not under test
    for (;;) {
      const { value, done } = await it.next();
      if (done) return;
      if (value.intent.case !== undefined) sentIntents.push(value);
    }
  }
  const backend = anInMemoryRpcBackend().implement(TddyRemote, {
    stream,
    getSession: async () => ({}),
    listSessions: async () => ({ sessions: [] }),
  });
  return { backend, sentIntents };
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("uses the provided placeholder for the chat input, with no SessionEntry dependency", () => {
  // Given / When — mounted with only a transport + a placeholder string (no session prop at all)
  const { backend } = aRecordingBackend();
  mountWithRpc(<AgentChat room={null} placeholder={PLACEHOLDER} />, backend);

  // Then
  agentChatPage.chat().should("exist");
  agentChatPage.chatInput().should("have.attr", "placeholder", PLACEHOLDER);
});

it("sends the operator's first message as a SubmitFeatureInput intent on a fresh connection", () => {
  // Given
  const { backend, sentIntents } = aRecordingBackend();
  mountWithRpc(<AgentChat room={null} placeholder={PLACEHOLDER} />, backend);

  // When
  agentChatPage.sendChatMessage("Add a health-check endpoint.");

  // Then
  cy.wrap(sentIntents).should((intents) => {
    expect(intents).to.have.length(1);
    expect(intents[0].intent.case).to.equal("submitFeatureInput");
    expect(intents[0].intent.value.text).to.equal("Add a health-check endpoint.");
  });
});

it("echoes the operator's sent message as a user chat bubble", () => {
  // Given
  const { backend } = aRecordingBackend();
  mountWithRpc(<AgentChat room={null} placeholder={PLACEHOLDER} />, backend);

  // When
  agentChatPage.sendChatMessage("Add a health-check endpoint.");

  // Then
  agentChatPage.chatMessage(0).should("exist").and("have.text", "Add a health-check endpoint.");
  agentChatPage.chatMessageKind(0).should("equal", "user");
});
