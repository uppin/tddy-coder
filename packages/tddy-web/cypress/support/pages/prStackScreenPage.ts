/**
 * Page object for the PR-Stack Chat Screen acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods.
 * No raw `cy.get(...)` in test files — only these named helpers.
 */

import {
  byTestId,
  prStackPlannedPrRow,
  prStackStartSessionBtn,
  prStackStatusChip,
  prStackChatMessage,
  prStackChatOption,
  prStackChatMultiSelectOption,
  TEST_IDS,
} from "../testIds";

export const prStackScreenPage = {
  // ---------------------------------------------------------------------------
  // Screen root
  // ---------------------------------------------------------------------------

  /** The PR-Stack Chat Screen root — rendered instead of the terminal for "pr-stack" sessions. */
  screen: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackScreen, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Planned-PR list
  // ---------------------------------------------------------------------------

  /** The list container for planned PRs. */
  plannedPrList: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackPlannedPrList, { timeout: 5000, ...options }),

  /** A single planned-PR row for the given stack node id. */
  plannedPrRow: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackPlannedPrRow(nodeId), { timeout: 5000, ...options }),

  /** The node ids of every rendered planned-PR row, in DOM order. */
  plannedPrRowNodeIds: (): Cypress.Chainable<string[]> =>
    prStackScreenPage
      .plannedPrList()
      .find("[data-testid^='pr-stack-planned-pr-row-']")
      .then(($rows) =>
        [...$rows].map((el) => el.getAttribute("data-testid")!.replace("pr-stack-planned-pr-row-", "")),
      ),

  /** The "Start session" CTA on an unspawned planned-PR row. */
  startSessionBtn: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackStartSessionBtn(nodeId), { timeout: 5000, ...options }),

  /** The status chip on an already-spawned planned-PR row. */
  statusChip: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackStatusChip(nodeId), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Chat window
  // ---------------------------------------------------------------------------

  /** The chat panel root. */
  chat: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChat, { timeout: 5000, ...options }),

  /** The scrollable message list. */
  chatMessages: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChatMessages, { timeout: 5000, ...options }),

  /** A single rendered chat bubble, in arrival order (0-indexed). */
  chatMessage: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackChatMessage(index), { timeout: 5000, ...options }),

  /** The bubble kind ("user" | "agent" | "goal" | "activity") of a chat bubble, in arrival order. */
  chatMessageKind: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackChatMessage(index), { timeout: 5000, ...options }).invoke("attr", "data-message-kind"),

  /** The chat text input. */
  chatInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChatInput, { timeout: 5000, ...options }),

  /** The chat send button. */
  chatSendBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChatSendBtn, { timeout: 5000, ...options }),

  /** The chat's inline error banner — connection failures, stream failures, or a rejected send. */
  chatError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChatError, { timeout: 5000, ...options }),

  /** The overlay shown while the presenter's own LiveKit room is still connecting. */
  chatConnectingOverlay: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChatConnecting, { timeout: 5000, ...options }),

  /** The persistent presenter connection status label (Not connected / Connecting… / Connected / Disconnected). */
  chatStatus: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChatStatus, { timeout: 5000, ...options }),

  /** The workflow-progress feedback line — surfaces goal/state changes so the panel is never silently empty while the agent works. */
  chatActivity: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChatActivity, { timeout: 5000, ...options }),

  /** Type a message into the chat input and click Send. */
  sendChatMessage(text: string) {
    byTestId(TEST_IDS.prStackChatInput).clear().type(text);
    byTestId(TEST_IDS.prStackChatSendBtn).click();
  },

  // ---------------------------------------------------------------------------
  // Clarification question elicitation (AppMode::Select / MultiSelect)
  // ---------------------------------------------------------------------------

  /** The clarification-question panel root, shown while the workflow awaits an answer. */
  chatQuestion: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChatQuestion, { timeout: 5000, ...options }),

  /** The question's header (short category label, e.g. "Backend"). */
  chatQuestionHeader: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChatQuestionHeader, { timeout: 5000, ...options }),

  /** The question's full text. */
  chatQuestionText: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChatQuestionText, { timeout: 5000, ...options }),

  /** A single-select option button, in option order (0-indexed). Clicking answers immediately. */
  chatOption: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackChatOption(index), { timeout: 5000, ...options }),

  /** A multi-select option checkbox, in option order (0-indexed). Toggling does not answer immediately. */
  chatMultiSelectOption: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackChatMultiSelectOption(index), { timeout: 5000, ...options }),

  /** Toggle a multi-select checkbox on. */
  toggleMultiSelectOption(index: number) {
    prStackScreenPage.chatMultiSelectOption(index).click();
  },

  /** Submit the checked multi-select options (and optional "Other" text). */
  submitMultiSelect() {
    byTestId(TEST_IDS.prStackChatMultiSelectSubmit).click();
  },

  /** The free-text "Other" input for a question that allows a custom answer. */
  chatQuestionOtherInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChatQuestionOtherInput, { timeout: 5000, ...options }),

  /** Type into the "Other" input without submitting (multi-select: submitted via `submitMultiSelect`). */
  typeOtherText(text: string) {
    byTestId(TEST_IDS.prStackChatQuestionOtherInput).clear().type(text);
  },

  /** Type and submit a custom "Other" answer for a single-select question. */
  answerOther(text: string) {
    prStackScreenPage.typeOtherText(text);
    byTestId(TEST_IDS.prStackChatQuestionOtherSubmit).click();
  },
};
