/**
 * Acceptance: the reusable `AgentChat`, mounted standalone, supports clarification-question
 * elicitation (`AppMode::Select` / `AppMode::MultiSelect`) — the same behavior the PR-Stack chat
 * had, now proven on the extracted component with no drawer / PrStackScreen in the tree.
 *
 * A `ModeChanged { select | multiSelect }` event replaces the free-text box with a question panel;
 * answering enqueues the matching `AnswerSelect` / `AnswerOther` / `AnswerMultiSelect` intent on the
 * open `TddyRemote.Stream`.
 *
 * PRD: docs/ft/web/session-drawer.md § Agent Chat.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AgentChat } from "../../src/components/chat/AgentChat";
import {
  TddyRemote,
  ServerMessageSchema,
  type ClientMessage,
  type ServerMessage,
  type ClarificationQuestionProto,
} from "../../src/gen/tddy/v1/remote_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentChatPage } from "../support/pages/agentChatPage";

function aClarificationQuestion(
  overrides: Partial<ClarificationQuestionProto> = {},
): ClarificationQuestionProto {
  return {
    header: "Backend",
    question: "Which coding backend should drive this session?",
    options: [
      { label: "Claude", description: "Anthropic's Claude Code CLI" },
      { label: "Cursor", description: "Cursor's composer agent" },
    ],
    multiSelect: false,
    allowOther: true,
    recommendedOther: "",
    ...overrides,
  } as ClarificationQuestionProto;
}

function selectModeMessage(question: ClarificationQuestionProto): ServerMessage {
  return create(ServerMessageSchema, {
    event: {
      case: "modeChanged",
      value: {
        mode: {
          variant: {
            case: "select",
            value: { question, questionIndex: 0, totalQuestions: 1, initialSelected: 0 },
          },
        },
      },
    },
  });
}

function multiSelectModeMessage(question: ClarificationQuestionProto): ServerMessage {
  return create(ServerMessageSchema, {
    event: {
      case: "modeChanged",
      value: {
        mode: { variant: { case: "multiSelect", value: { question, questionIndex: 0, totalQuestions: 1 } } },
      },
    },
  });
}

/**
 * A backend that yields `initialMessage` on stream open and records every intent the operator sends
 * afterward, so the test can assert the exact `Client` intent produced by answering.
 */
function aScriptedQuestionBackend(initialMessage: ServerMessage) {
  const sentIntents: ClientMessage[] = [];
  async function* stream(requests: AsyncIterable<ClientMessage>) {
    const it = requests[Symbol.asyncIterator]();
    await it.next(); // eager empty open frame — no intent, not under test
    yield initialMessage;
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

it("renders a pending select question's header, text, and options and hides the free-text input", () => {
  // Given / When
  const { backend } = aScriptedQuestionBackend(selectModeMessage(aClarificationQuestion()));
  mountWithRpc(<AgentChat room={null} placeholder="Message the agent…" />, backend);

  // Then
  agentChatPage.chatQuestion().should("exist");
  agentChatPage.chatQuestionHeader().should("contain.text", "Backend");
  agentChatPage
    .chatQuestionText()
    .should("contain.text", "Which coding backend should drive this session?");
  agentChatPage.chatOption(0).should("contain.text", "Claude");
  agentChatPage.chatOption(1).should("contain.text", "Cursor");
  agentChatPage.chatInput().should("not.exist");
});

it("sends an AnswerSelect intent with the chosen option's index when an option is clicked", () => {
  // Given
  const { backend, sentIntents } = aScriptedQuestionBackend(
    selectModeMessage(aClarificationQuestion()),
  );
  mountWithRpc(<AgentChat room={null} placeholder="Message the agent…" />, backend);

  // When
  agentChatPage.chatOption(1).click();

  // Then
  cy.wrap(sentIntents).should((intents) => {
    expect(intents).to.have.length(1);
    expect(intents[0].intent.case).to.equal("answerSelect");
    expect(intents[0].intent.value.index).to.equal(1);
  });
});

it("sends an AnswerOther intent with the typed text when a custom answer is submitted", () => {
  // Given
  const { backend, sentIntents } = aScriptedQuestionBackend(
    selectModeMessage(aClarificationQuestion({ allowOther: true })),
  );
  mountWithRpc(<AgentChat room={null} placeholder="Message the agent…" />, backend);

  // When
  agentChatPage.answerOther("Codex, actually.");

  // Then
  cy.wrap(sentIntents).should((intents) => {
    expect(intents).to.have.length(1);
    expect(intents[0].intent.case).to.equal("answerOther");
    expect(intents[0].intent.value.text).to.equal("Codex, actually.");
  });
});

it("sends an AnswerMultiSelect intent with all checked indices when Submit is clicked", () => {
  // Given
  const question = aClarificationQuestion({
    header: "Reviewers",
    question: "Which reviewers should be requested?",
    options: [
      { label: "Alice", description: "" },
      { label: "Bob", description: "" },
      { label: "Carol", description: "" },
    ],
    multiSelect: true,
  });
  const { backend, sentIntents } = aScriptedQuestionBackend(multiSelectModeMessage(question));
  mountWithRpc(<AgentChat room={null} placeholder="Message the agent…" />, backend);

  // When — check the first and third options, leave the second unchecked
  agentChatPage.toggleMultiSelectOption(0);
  agentChatPage.toggleMultiSelectOption(2);
  agentChatPage.submitMultiSelect();

  // Then
  cy.wrap(sentIntents).should((intents) => {
    expect(intents).to.have.length(1);
    expect(intents[0].intent.case).to.equal("answerMultiSelect");
    expect(intents[0].intent.value.indices).to.deep.equal([0, 2]);
    expect(intents[0].intent.value.other).to.equal("");
  });
});
