/**
 * Centralised data-testid constants.
 *
 * Every `data-testid` value used by the Cypress suite lives here — the raw string
 * appears once; tests use the named constant so a rename is a one-line change.
 *
 * For identifiers that include a dynamic segment (session ID, project ID, …) the
 * constant is a prefix or a helper function — see the examples below.
 */

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/** Build a `cy.get` selector for `[data-testid='<id>']`. */
export const byTestId = (
  id: string,
  options?: Parameters<typeof cy.get>[1],
): Cypress.Chainable<JQuery<HTMLElement>> =>
  cy.get(`[data-testid='${id}']`, options);

// ---------------------------------------------------------------------------
// Session runtime and terminal
// ---------------------------------------------------------------------------

export const TEST_IDS = {
  controlHolderDisplay: "control-holder-display",
  controlIsControllerDisplay: "control-is-controller-display",
  controlTokenDisplay: "control-token-display",
  enqueuedInputOverlay: "enqueued-input-overlay",
  enqueuedInputText: "enqueued-input-text",
  ghosttyTerminal: "ghostty-terminal",
  loadEarlierHistory: "load-earlier-history",
  sessionConnectionError: "session-connection-error",
  sessionConnectionOverlay: "session-connection-overlay",
  switchSession: "switch-session",
  terminalBufferText: "terminal-buffer-text",
  terminalClaimBtn: "terminal-claim-btn",
  terminalControlHolder: "terminal-control-holder",
  terminalControlOverlay: "terminal-control-overlay",
  terminalInputError: "terminal-input-error",
  terminalOlderBufferText: "terminal-older-buffer-text",
} as const;
