/**
 * Page object for the App component auth screen and LiveKit connection form.
 *
 * Covers both the GitHub OAuth login entry point and the legacy direct-connect form
 * (`#livekit-url`, `#livekit-room`, etc.).
 */

import { byTestId, TEST_IDS } from "../testIds";

export const appAuthPage = {
  // ---------------------------------------------------------------------------
  // Auth
  // ---------------------------------------------------------------------------

  /** GitHub login button. */
  loginButton: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.githubLoginButton, { timeout: 10000, ...options }),

  /** User identity shown after successful login. */
  userLogin: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.userLogin, { timeout: 15000, ...options }),

  // ---------------------------------------------------------------------------
  // Legacy LiveKit direct-connect form
  // ---------------------------------------------------------------------------

  /** LiveKit server URL input (`#livekit-url`). */
  livekitUrlInput: (options?: Parameters<typeof cy.get>[1]) =>
    cy.get("#livekit-url", options),

  /** LiveKit room input (`#livekit-room`). */
  livekitRoomInput: (options?: Parameters<typeof cy.get>[1]) =>
    cy.get("#livekit-room", options),

  /** LiveKit identity input. */
  livekitIdentityInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.livekitIdentity, options),

  /** Submit button for the LiveKit connect form. */
  submitButton: (options?: Parameters<typeof cy.get>[1]) =>
    cy.get("button[type='submit']", options),

  // ---------------------------------------------------------------------------
  // Post-connection
  // ---------------------------------------------------------------------------

  /** Connection status dot (terminal chrome). */
  statusDot: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.connectionStatusDot, { timeout: 15000, ...options }),

  /** Ctrl-C button (terminal chrome). */
  ctrlCButton: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.ctrlCButton, options),

  /** Terminal container shown after a successful connect. */
  terminalContainer: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.connectedTerminalContainer, { timeout: 15000, ...options }),

  /** Build-id label in the connection chrome. */
  buildId: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.buildId, options),

  /** Mobile keyboard overlay button. */
  mobileKeyboardButton: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.mobileKeyboardButton, options),
};
