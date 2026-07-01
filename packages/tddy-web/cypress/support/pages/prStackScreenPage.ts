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

  /** The chat text input. */
  chatInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChatInput, { timeout: 5000, ...options }),

  /** The chat send button. */
  chatSendBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.prStackChatSendBtn, { timeout: 5000, ...options }),

  /** Type a message into the chat input and click Send. */
  sendChatMessage(text: string) {
    byTestId(TEST_IDS.prStackChatInput).clear().type(text);
    byTestId(TEST_IDS.prStackChatSendBtn).click();
  },
};
