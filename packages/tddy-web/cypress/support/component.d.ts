/// <reference types="cypress" />

declare global {
  namespace Cypress {
    interface Chainable {
      mountSessionTablesDemo(options: {
        outerWidthPx: number;
        viewport?: [number, number];
      }): Chainable<JQuery<HTMLElement>>;
    }
  }
}

export {};
