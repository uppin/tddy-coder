/**
 * Acceptance: TerminalView Storybook stories render without errors.
 *
 * Stories: Default, WithContent, ColorPalette
 */
import { byTestId, TEST_IDS } from "../support/testIds";

describe("TerminalView Stories", () => {
  it("renders Default story with an empty terminal screen", () => {
    // Given / When
    cy.visit("/iframe.html?id=components-terminal--default&viewMode=story");

    // Then
    byTestId(TEST_IDS.terminal, { timeout: 10000 }).should("exist");
  });

  it("renders WithContent story showing ANSI-colored output", () => {
    // Given / When
    cy.visit("/iframe.html?id=components-terminal--with-content&viewMode=story");

    // Then
    byTestId(TEST_IDS.terminal, { timeout: 10000 }).should("exist");
  });

  it("renders ColorPalette story", () => {
    // Given / When
    cy.visit("/iframe.html?id=components-terminal--color-palette&viewMode=story");

    // Then
    byTestId(TEST_IDS.terminal, { timeout: 10000 }).should("exist");
  });
});
