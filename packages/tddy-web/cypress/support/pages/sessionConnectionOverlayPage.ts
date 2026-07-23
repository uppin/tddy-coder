/**
 * Page object for the session connection overlay component tests.
 *
 * All raw selectors live here; test bodies call named methods.
 * No raw `cy.get(...)` in test files — only these named helpers.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const sessionConnectionOverlayPage = {
  /** The overlay that covers a session's panes while it is not yet connected to LiveKit. */
  overlay: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionConnectionOverlay, { timeout: 5000, ...options }),

  /** The error message shown inside the overlay when the LiveKit connection fails. */
  error: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionConnectionError, { timeout: 5000, ...options }),
};
