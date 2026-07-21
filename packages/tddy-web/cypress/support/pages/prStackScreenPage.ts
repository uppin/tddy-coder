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
  prStackInternalStatusBadge,
  agentChatMessage,
  agentChatOption,
  agentChatMultiSelectOption,
  prStackAddPlannedPrAncestorCheckbox,
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

  /** The action-needed internal-status badge on a planned-PR row (e.g. "needs-repoint"). */
  internalStatusBadge: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackInternalStatusBadge(nodeId), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Manually adding a planned PR (deterministic, non-chat path)
  // ---------------------------------------------------------------------------

  /** The "+ New planned PR" entry-point button that opens the add-planned-PR form. */
  addPlannedPrBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrBtn, { timeout: 5000, ...options }),

  /** The add-planned-PR form root. */
  addPlannedPrForm: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrForm, { timeout: 5000, ...options }),

  /** The new planned PR's title input. */
  addPlannedPrTitleInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrTitleInput, { timeout: 5000, ...options }),

  /** The new planned PR's description input. */
  addPlannedPrDescriptionInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrDescriptionInput, { timeout: 5000, ...options }),

  /** The new planned PR's optional branch-suggestion input. */
  addPlannedPrBranchSuggestionInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrBranchSuggestionInput, { timeout: 5000, ...options }),

  /** An ancestor checkbox for the given existing planned-PR node id. */
  addPlannedPrAncestorCheckbox: (nodeId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(prStackAddPlannedPrAncestorCheckbox(nodeId), { timeout: 5000, ...options }),

  /** Submit button for the add-planned-PR form. */
  addPlannedPrSubmitBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrSubmitBtn, { timeout: 5000, ...options }),

  /** Cancel button for the add-planned-PR form. */
  addPlannedPrCancelBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrCancelBtn, { timeout: 5000, ...options }),

  /** Inline error banner shown when adding a planned PR fails. */
  addPlannedPrError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackAddPlannedPrError, { timeout: 5000, ...options }),

  /** Open the "New planned PR" form. */
  openAddPlannedPrForm() {
    byTestId(TEST_IDS.prStackAddPlannedPrBtn).click();
  },

  /**
   * Fill and submit the "New planned PR" form: types the title (and optional description /
   * branch suggestion), checks the given ancestor node ids, then clicks submit. Assumes the
   * form is already open.
   */
  fillAndSubmitAddPlannedPrForm(options: {
    title: string;
    description?: string;
    branchSuggestion?: string;
    ancestorNodeIds?: string[];
  }) {
    byTestId(TEST_IDS.prStackAddPlannedPrTitleInput).clear().type(options.title);
    if (options.description) {
      byTestId(TEST_IDS.prStackAddPlannedPrDescriptionInput).clear().type(options.description);
    }
    if (options.branchSuggestion) {
      byTestId(TEST_IDS.prStackAddPlannedPrBranchSuggestionInput)
        .clear()
        .type(options.branchSuggestion);
    }
    for (const nodeId of options.ancestorNodeIds ?? []) {
      byTestId(prStackAddPlannedPrAncestorCheckbox(nodeId)).click();
    }
    byTestId(TEST_IDS.prStackAddPlannedPrSubmitBtn).click();
  },

  // ---------------------------------------------------------------------------
  // Chat window
  // ---------------------------------------------------------------------------

  /** The chat panel root. */
  chat: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChat, { timeout: 5000, ...options }),

  /** The scrollable message list. */
  chatMessages: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatMessages, { timeout: 5000, ...options }),

  /** A single rendered chat bubble, in arrival order (0-indexed). */
  chatMessage: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatMessage(index), { timeout: 5000, ...options }),

  /** The bubble kind ("user" | "agent" | "goal" | "activity") of a chat bubble, in arrival order. */
  chatMessageKind: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatMessage(index), { timeout: 5000, ...options }).invoke("attr", "data-message-kind"),

  /** The chat text input. */
  chatInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatInput, { timeout: 5000, ...options }),

  /** The chat send button. */
  chatSendBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatSendBtn, { timeout: 5000, ...options }),

  /** The chat's inline error banner — connection failures, stream failures, or a rejected send. */
  chatError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatError, { timeout: 5000, ...options }),

  /** The overlay shown while the presenter's own LiveKit room is still connecting. */
  chatConnectingOverlay: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatConnecting, { timeout: 5000, ...options }),

  /** The persistent presenter connection status label (Not connected / Connecting… / Connected / Disconnected). */
  chatStatus: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatStatus, { timeout: 5000, ...options }),

  /** Type a message into the chat input and click Send. */
  sendChatMessage(text: string) {
    byTestId(TEST_IDS.agentChatInput).clear().type(text);
    byTestId(TEST_IDS.agentChatSendBtn).click();
  },

  // ---------------------------------------------------------------------------
  // Clarification question elicitation (AppMode::Select / MultiSelect)
  // ---------------------------------------------------------------------------

  /** The clarification-question panel root, shown while the workflow awaits an answer. */
  chatQuestion: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatQuestion, { timeout: 5000, ...options }),

  /** The question's header (short category label, e.g. "Backend"). */
  chatQuestionHeader: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatQuestionHeader, { timeout: 5000, ...options }),

  /** The question's full text. */
  chatQuestionText: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatQuestionText, { timeout: 5000, ...options }),

  /** A single-select option button, in option order (0-indexed). Clicking answers immediately. */
  chatOption: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatOption(index), { timeout: 5000, ...options }),

  /** A multi-select option checkbox, in option order (0-indexed). Toggling does not answer immediately. */
  chatMultiSelectOption: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatMultiSelectOption(index), { timeout: 5000, ...options }),

  /** Toggle a multi-select checkbox on. */
  toggleMultiSelectOption(index: number) {
    prStackScreenPage.chatMultiSelectOption(index).click();
  },

  /** Submit the checked multi-select options (and optional "Other" text). */
  submitMultiSelect() {
    byTestId(TEST_IDS.agentChatMultiSelectSubmit).click();
  },

  /** The free-text "Other" input for a question that allows a custom answer. */
  chatQuestionOtherInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatQuestionOtherInput, { timeout: 5000, ...options }),

  /** Type into the "Other" input without submitting (multi-select: submitted via `submitMultiSelect`). */
  typeOtherText(text: string) {
    byTestId(TEST_IDS.agentChatQuestionOtherInput).clear().type(text);
  },

  /** Type and submit a custom "Other" answer for a single-select question. */
  answerOther(text: string) {
    prStackScreenPage.typeOtherText(text);
    byTestId(TEST_IDS.agentChatQuestionOtherSubmit).click();
  },

  // ---------------------------------------------------------------------------
  // Session-creation dialog (opened by the "Start session" CTA)
  // ---------------------------------------------------------------------------

  /** The overlay dialog wrapping the reused `CreateSessionPane`. */
  createSessionDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionDialog, { timeout: 5000, ...options }),

  /** The reused `CreateSessionPane` rendered inside the dialog. */
  createSessionPaneInDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionPane, { timeout: 5000, ...options }),

  /** The dialog's new-branch-name input (pre-filled from the planned PR's branch). */
  dialogNewBranchNameInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionNewBranchNameInput, { timeout: 5000, ...options }),

  /** The dialog's initial-prompt textarea (pre-filled from the planned PR's title + description). */
  dialogInitialPromptInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionInitialPromptInput, { timeout: 5000, ...options }),

  /** The dialog's Create button. */
  dialogSubmitBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionSubmitBtn, { timeout: 5000, ...options }),
};
