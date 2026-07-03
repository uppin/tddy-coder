/**
 * Acceptance tests: the PR-Stack Chat Screen must support clarification-question elicitation
 * (`AppMode::Select` / `AppMode::MultiSelect`), not just free-text chat.
 *
 * Bug reproduced live: when the workflow paused awaiting an `AskUserQuestion` answer, the chat
 * UI kept showing the free-text box. The operator's reply was sent as `QueuePrompt`, which the
 * presenter only drains into an inbox — it never satisfies the pending `AnswerSelect`/
 * `AnswerOther`/`AnswerMultiSelect` the workflow thread is blocked on, so the workflow hangs
 * forever with no further `agentOutput` or `modeChanged` event
 * (`packages/tddy-core/src/presenter/presenter_impl.rs`, `QueuePrompt` handler, lines ~637-656).
 *
 * The wire protocol already carries everything needed end-to-end (`AppModeSelect`/
 * `AppModeMultiSelect`/`ClarificationQuestionProto` on the way in; `AnswerSelect`/`AnswerOther`/
 * `AnswerMultiSelect` on the way out) — this was purely a web UI gap.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views (chat window).
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import {
  TddyRemote,
  ServerMessageSchema,
  type ClientMessage,
  type ServerMessage,
  type ClarificationQuestionProto,
} from "../../src/gen/tddy/v1/remote_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PR_STACK_SESSION = {
  sessionId: "pr-stack-elicitation-0000-0000-0000-000000000001",
  createdAt: "2026-07-03T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  pid: 0,
  isActive: false,
  projectId: "proj-pr-stack-elicitation",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "pr-stack",
  stackPlanJson: "",
};

function aClarificationQuestion(
  overrides: Partial<ClarificationQuestionProto> = {},
): ClarificationQuestionProto {
  return {
    header: "Backend",
    question: "Which coding backend should drive this PR stack?",
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
        mode: {
          variant: {
            case: "multiSelect",
            value: { question, questionIndex: 0, totalQuestions: 1 },
          },
        },
      },
    },
  });
}

function runningModeMessage(): ServerMessage {
  return create(ServerMessageSchema, {
    event: { case: "modeChanged", value: { mode: { variant: { case: "running", value: {} } } } },
  });
}

/**
 * A backend that immediately yields `initialMessage` on stream open, records every intent the
 * operator sends afterward, and optionally reacts to a recorded intent with a follow-up
 * `ServerMessage` (e.g. the workflow resuming to `Running` after the question is answered).
 */
function aScriptedQuestionBackend(
  initialMessage: ServerMessage,
  onIntent?: (intent: ClientMessage) => ServerMessage | undefined,
) {
  const sentIntents: ClientMessage[] = [];
  async function* stream(requests: AsyncIterable<ClientMessage>) {
    const it = requests[Symbol.asyncIterator]();
    await it.next(); // eager empty open frame — no intent, not under test
    yield initialMessage;
    for (;;) {
      const { value, done } = await it.next();
      if (done) return;
      if (value.intent.case !== undefined) sentIntents.push(value);
      const follow = onIntent?.(value);
      if (follow) yield follow;
    }
  }
  const backend = aSessionsDrawerBackend([PR_STACK_SESSION]).implement(TddyRemote, {
    stream,
    getSession: async () => ({}),
    listSessions: async () => ({ sessions: [] }),
  });
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
// Select mode — rendering
// ---------------------------------------------------------------------------

it("renders the clarification question's header, text, and options when the presenter enters Select mode", () => {
  // Given
  const { backend } = aScriptedQuestionBackend(selectModeMessage(aClarificationQuestion()));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.chatQuestion().should("exist");
  prStackScreenPage.chatQuestionHeader().should("contain.text", "Backend");
  prStackScreenPage
    .chatQuestionText()
    .should("contain.text", "Which coding backend should drive this PR stack?");
  prStackScreenPage.chatOption(0).should("contain.text", "Claude");
  prStackScreenPage.chatOption(1).should("contain.text", "Cursor");
});

it("hides the free-text chat input while a select question is pending", () => {
  // Given / When — this is the exact bug: typing here would send QueuePrompt, which the
  // presenter drains into an inbox instead of answering the pending question
  const { backend } = aScriptedQuestionBackend(selectModeMessage(aClarificationQuestion()));
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.chatQuestion().should("exist");
  prStackScreenPage.chatInput().should("not.exist");
});

// ---------------------------------------------------------------------------
// Select mode — answering
// ---------------------------------------------------------------------------

it("sends an AnswerSelect intent with the chosen option's index when an option is clicked", () => {
  // Given
  const { backend, sentIntents } = aScriptedQuestionBackend(
    selectModeMessage(aClarificationQuestion()),
  );
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // When
  prStackScreenPage.chatOption(1).click();

  // Then
  cy.wrap(sentIntents).should((intents) => {
    expect(intents).to.have.length(1);
    expect(intents[0].intent.case).to.equal("answerSelect");
    expect(intents[0].intent.value.index).to.equal(1);
  });
});

it("echoes the chosen option's label as a user chat bubble after answering a select question", () => {
  // Given
  const { backend } = aScriptedQuestionBackend(selectModeMessage(aClarificationQuestion()));
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // When
  prStackScreenPage.chatOption(1).click();

  // Then
  prStackScreenPage.chatMessage(0).should("exist").and("contain.text", "Cursor");
});

it("restores the free-text chat input after the workflow resumes running following an answered question", () => {
  // Given — the presenter resumes Running once the operator answers
  const { backend } = aScriptedQuestionBackend(
    selectModeMessage(aClarificationQuestion()),
    (intent) => (intent.intent.case === "answerSelect" ? runningModeMessage() : undefined),
  );
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // When
  prStackScreenPage.chatOption(0).click();

  // Then
  prStackScreenPage.chatQuestion().should("not.exist");
  prStackScreenPage.chatInput().should("exist");
});

// ---------------------------------------------------------------------------
// Select mode — "Other" custom answer
// ---------------------------------------------------------------------------

it("renders an Other input for a select question that allows a custom answer", () => {
  // Given / When
  const { backend } = aScriptedQuestionBackend(
    selectModeMessage(aClarificationQuestion({ allowOther: true })),
  );
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.chatQuestionOtherInput().should("exist");
});

it("does not render an Other input for a select question that disallows a custom answer", () => {
  // Given / When
  const { backend } = aScriptedQuestionBackend(
    selectModeMessage(aClarificationQuestion({ allowOther: false })),
  );
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.chatQuestionOtherInput().should("not.exist");
});

it("sends an AnswerOther intent with the typed text when a custom answer is submitted", () => {
  // Given
  const { backend, sentIntents } = aScriptedQuestionBackend(
    selectModeMessage(aClarificationQuestion({ allowOther: true })),
  );
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // When
  prStackScreenPage.answerOther("Codex, actually.");

  // Then
  cy.wrap(sentIntents).should((intents) => {
    expect(intents).to.have.length(1);
    expect(intents[0].intent.case).to.equal("answerOther");
    expect(intents[0].intent.value.text).to.equal("Codex, actually.");
  });
});

// ---------------------------------------------------------------------------
// MultiSelect mode
// ---------------------------------------------------------------------------

it("renders the header, text, and a checkbox per option for a multi-select clarification question", () => {
  // Given
  const question = aClarificationQuestion({
    header: "Reviewers",
    question: "Which reviewers should be requested on every PR in the stack?",
    options: [
      { label: "Alice", description: "Backend owner" },
      { label: "Bob", description: "Frontend owner" },
      { label: "Carol", description: "Infra owner" },
    ],
    multiSelect: true,
  });
  const { backend } = aScriptedQuestionBackend(multiSelectModeMessage(question));

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.chatQuestionHeader().should("contain.text", "Reviewers");
  prStackScreenPage
    .chatQuestionText()
    .should("contain.text", "Which reviewers should be requested on every PR in the stack?");
  prStackScreenPage.chatMultiSelectOption(0).should("contain.text", "Alice");
  prStackScreenPage.chatMultiSelectOption(1).should("contain.text", "Bob");
  prStackScreenPage.chatMultiSelectOption(2).should("contain.text", "Carol");
});

it("sends an AnswerMultiSelect intent with all checked indices when Submit is clicked", () => {
  // Given
  const question = aClarificationQuestion({
    options: [
      { label: "Alice", description: "" },
      { label: "Bob", description: "" },
      { label: "Carol", description: "" },
    ],
    multiSelect: true,
  });
  const { backend, sentIntents } = aScriptedQuestionBackend(multiSelectModeMessage(question));
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // When — check the first and third options, leave the second unchecked
  prStackScreenPage.toggleMultiSelectOption(0);
  prStackScreenPage.toggleMultiSelectOption(2);
  prStackScreenPage.submitMultiSelect();

  // Then
  cy.wrap(sentIntents).should((intents) => {
    expect(intents).to.have.length(1);
    expect(intents[0].intent.case).to.equal("answerMultiSelect");
    expect(intents[0].intent.value.indices).to.deep.equal([0, 2]);
    expect(intents[0].intent.value.other).to.equal("");
  });
});

it("includes the typed Other text in the AnswerMultiSelect intent when submitted", () => {
  // Given
  const question = aClarificationQuestion({
    options: [
      { label: "Alice", description: "" },
      { label: "Bob", description: "" },
    ],
    multiSelect: true,
    allowOther: true,
  });
  const { backend, sentIntents } = aScriptedQuestionBackend(multiSelectModeMessage(question));
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // When
  prStackScreenPage.toggleMultiSelectOption(1);
  prStackScreenPage.typeOtherText("Dave too, he's on-call.");
  prStackScreenPage.submitMultiSelect();

  // Then
  cy.wrap(sentIntents).should((intents) => {
    expect(intents).to.have.length(1);
    expect(intents[0].intent.case).to.equal("answerMultiSelect");
    expect(intents[0].intent.value.indices).to.deep.equal([1]);
    expect(intents[0].intent.value.other).to.equal("Dave too, he's on-call.");
  });
});

it("does not render an Other input for a multi-select question that disallows a custom answer", () => {
  // Given / When
  const question = aClarificationQuestion({ multiSelect: true, allowOther: false });
  const { backend } = aScriptedQuestionBackend(multiSelectModeMessage(question));
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.chatQuestionOtherInput().should("not.exist");
});
