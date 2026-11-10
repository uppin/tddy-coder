/**
 * Page object for the terminal-control toast a session runtime shows when this screen is not the
 * terminal's controller. All raw selectors live here; specs call named methods.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const terminalControlPage = {
  /** The toast that appears when this screen is not the terminal controller. */
  terminalControlOverlay: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalControlOverlay, { timeout: 5000, ...options }),

  /** The "Claim terminal" button inside the control toast. */
  terminalClaimBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalClaimBtn, { timeout: 5000, ...options }),

  /** The text element naming the screen currently holding control. */
  terminalControlHolder: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalControlHolder, { timeout: 5000, ...options }),
};
