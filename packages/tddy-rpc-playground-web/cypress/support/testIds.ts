/**
 * Centralised data-testid constants for this package's Cypress suite.
 *
 * Every `data-testid` value used here lives once; tests use the named constant so a rename is a
 * one-line change. Only the RPC Playground ids live in this package — the app shell's ids stay
 * with `tddy-web`.
 */

/** Build a `cy.get` selector for `[data-testid='<id>']`. */
export const byTestId = (
  id: string,
  options?: Parameters<typeof cy.get>[1],
): Cypress.Chainable<JQuery<HTMLElement>> =>
  cy.get(`[data-testid='${id}']`, options);

export const TEST_IDS = {
  // RPC Playground
  rpcPlaygroundParticipantSelect: "rpc-playground-participant-select",
  /** Replaces the participant picker when the connection carries no LiveKit presence. */
  rpcPlaygroundParticipantUnavailable: "rpc-playground-participant-unavailable",
  rpcServiceTree: "rpc-service-tree",
  rpcRequestEditor: "rpc-request-editor",
  rpcRequestRawJson: "rpc-request-raw-json",
  rpcEditorToggleRaw: "rpc-editor-toggle-raw",
  rpcEditorToggleBuilder: "rpc-editor-toggle-builder",
  rpcInvokeButton: "rpc-invoke-button",
  rpcResponse: "rpc-response",
  rpcError: "rpc-error",
} as const;
