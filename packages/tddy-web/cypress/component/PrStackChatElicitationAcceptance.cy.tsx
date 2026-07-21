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
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { TddyRemote } from "../../src/gen/tddy/v1/remote_pb";
import { AcpService, type AcpAgentMessage, type AcpClientMessage } from "../../src/gen/tddy/acp/v1/acp_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { acpQuestion, selectedOptionId } from "../support/rpc/acpSession";
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

interface QuestionSpec {
  header: string;
  question: string;
  labels: string[];
  multiSelect: boolean;
  allowOther: boolean;
}

function aClarificationQuestion(overrides: Partial<QuestionSpec> = {}): QuestionSpec {
  return {
    header: "Backend",
    question: "Which coding backend should drive this PR stack?",
    labels: ["Claude", "Cursor"],
    multiSelect: false,
    allowOther: true,
    ...overrides,
  };
}

/** The agent's `request_permission` for a (single- or multi-select) clarification. */
function questionMessage(q: QuestionSpec): AcpAgentMessage {
  return acpQuestion(q.labels, {
    multi: q.multiSelect,
    allowOther: q.allowOther,
    header: q.header,
    question: q.question,
  });
}

/**
 * A backend that yields the clarification's `request_permission` on stream open, records every
 * prompt/permission reply the operator sends, and optionally reacts to a reply with a follow-up
 * `AcpAgentMessage`.
 */
function aScriptedQuestionBackend(
  initialMessage: AcpAgentMessage,
  onReply?: (reply: AcpClientMessage) => AcpAgentMessage | undefined,
) {
  const sentIntents: AcpClientMessage[] = [];
  async function* session(requests: AsyncIterable<AcpClientMessage>) {
    yield initialMessage;
    for await (const req of requests) {
      const c = req.msg.case;
      if (c !== "prompt" && c !== "requestPermission") continue;
      sentIntents.push(req);
      const follow = onReply?.(req);
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
// Select mode — rendering
// ---------------------------------------------------------------------------

it("renders the clarification question's header, text, and options when the presenter enters Select mode", () => {
  // Given
  const { backend } = aScriptedQuestionBackend(questionMessage(aClarificationQuestion()));

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
  const { backend } = aScriptedQuestionBackend(questionMessage(aClarificationQuestion()));
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
    questionMessage(aClarificationQuestion()),
  );
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // When
  prStackScreenPage.chatOption(1).click();

  // Then — the reply selects option-1 (the agent decodes it to AnswerSelect(1)).
  cy.wrap(sentIntents).should((sent) => {
    expect(sent).to.have.length(1);
    expect(selectedOptionId(sent[0])).to.equal("option-1");
  });
});

it("echoes the chosen option's label as a user chat bubble after answering a select question", () => {
  // Given
  const { backend } = aScriptedQuestionBackend(questionMessage(aClarificationQuestion()));
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // When
  prStackScreenPage.chatOption(1).click();

  // Then
  prStackScreenPage.chatMessage(0).should("exist").and("contain.text", "Cursor");
});

it("restores the free-text chat input after answering a question", () => {
  // Given — a pending clarification (answering clears it client-side, restoring the free-text box)
  const { backend } = aScriptedQuestionBackend(questionMessage(aClarificationQuestion()));
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
    questionMessage(aClarificationQuestion({ allowOther: true })),
  );
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.chatQuestionOtherInput().should("exist");
});

it("does not render an Other input for a select question that disallows a custom answer", () => {
  // Given / When
  const { backend } = aScriptedQuestionBackend(
    questionMessage(aClarificationQuestion({ allowOther: false })),
  );
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.chatQuestionOtherInput().should("not.exist");
});

it("sends an AnswerOther intent with the typed text when a custom answer is submitted", () => {
  // Given
  const { backend, sentIntents } = aScriptedQuestionBackend(
    questionMessage(aClarificationQuestion({ allowOther: true })),
  );
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // When
  prStackScreenPage.answerOther("Codex, actually.");

  // Then — the custom answer rides the option id (agent decodes it to AnswerOther(text)).
  cy.wrap(sentIntents).should((sent) => {
    expect(sent).to.have.length(1);
    expect(selectedOptionId(sent[0])).to.equal("other:Codex, actually.");
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
    labels: ["Alice", "Bob", "Carol"],
    multiSelect: true,
  });
  const { backend } = aScriptedQuestionBackend(questionMessage(question));

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
    labels: ["Alice", "Bob", "Carol"],
    multiSelect: true,
  });
  const { backend, sentIntents } = aScriptedQuestionBackend(questionMessage(question));
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // When — check the first and third options, leave the second unchecked
  prStackScreenPage.toggleMultiSelectOption(0);
  prStackScreenPage.toggleMultiSelectOption(2);
  prStackScreenPage.submitMultiSelect();

  // Then — the reply encodes the checked indices (agent decodes to AnswerMultiSelect([0,2])).
  cy.wrap(sentIntents).should((sent) => {
    expect(sent).to.have.length(1);
    expect(selectedOptionId(sent[0])).to.equal("multi:0,2");
  });
});

it("includes the typed Other text in the AnswerMultiSelect intent when submitted", () => {
  // Given
  const question = aClarificationQuestion({
    labels: ["Alice", "Bob"],
    multiSelect: true,
    allowOther: true,
  });
  const { backend, sentIntents } = aScriptedQuestionBackend(questionMessage(question));
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // When
  prStackScreenPage.toggleMultiSelectOption(1);
  prStackScreenPage.typeOtherText("Dave too, he's on-call.");
  prStackScreenPage.submitMultiSelect();

  // Then — the checked index + custom text ride the option id (agent decodes to AnswerMultiSelect).
  cy.wrap(sentIntents).should((sent) => {
    expect(sent).to.have.length(1);
    expect(selectedOptionId(sent[0])).to.equal("multi:1;other=Dave too, he's on-call.");
  });
});

it("does not render an Other input for a multi-select question that disallows a custom answer", () => {
  // Given / When
  const question = aClarificationQuestion({ multiSelect: true, allowOther: false });
  const { backend } = aScriptedQuestionBackend(questionMessage(question));
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.chatQuestionOtherInput().should("not.exist");
});
