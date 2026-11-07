/** `data-testid` constants for this package's specs — each raw string appears once. */

/** Build a `cy.get` selector for `[data-testid='<id>']`. */
export const byTestId = (
  id: string,
  options?: Parameters<typeof cy.get>[1],
): Cypress.Chainable<JQuery<HTMLElement>> => cy.get(`[data-testid='${id}']`, options);

export const TEST_IDS = {
  // Visual viewport consumer (test harness)
  viewportConsumer: "viewport-consumer",
  viewportHeight: "viewport-height",
  viewportKeyboardOpen: "viewport-keyboard-open",
} as const;
