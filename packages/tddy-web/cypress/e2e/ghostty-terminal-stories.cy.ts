/**
 * Acceptance: GhosttyTerminal Storybook stories render without errors.
 *
 * Stories: Default, WithContent, ColorPalette
 */
import { byTestId, TEST_IDS } from "../support/testIds";

describe("GhosttyTerminal Stories", () => {
  it("renders Default story with an empty terminal canvas", () => {
    // Given / When
    cy.visit("/iframe.html?id=components-ghosttyterminal--default&viewMode=story");

    // Then
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");
  });

  it("renders WithContent story showing ANSI-colored output", () => {
    // Given / When
    cy.visit("/iframe.html?id=components-ghosttyterminal--with-content&viewMode=story");

    // Then
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");
  });

  it("renders ColorPalette story", () => {
    // Given / When
    cy.visit("/iframe.html?id=components-ghosttyterminal--color-palette&viewMode=story");

    // Then
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");
  });
});
