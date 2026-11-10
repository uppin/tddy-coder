/**
 * Page object for the full-screen Workflow Chat Screen acceptance tests — the single-pane chat view
 * rendered instead of the terminal for every non-"pr-stack" tddy-coder `tool` workflow session.
 *
 * All raw selectors live here; test bodies call named methods.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const workflowChatScreenPage = {
  /** The Workflow Chat Screen root — rendered instead of the terminal for tool workflow sessions. */
  screen: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.workflowChatScreen, { timeout: 5000, ...options }),

  /** The reusable chat panel mounted inside the screen. */
  chat: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChat, { timeout: 5000, ...options }),

  /** The chat text input. */
  chatInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatInput, { timeout: 5000, ...options }),
};
