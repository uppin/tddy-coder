/**
 * Acceptance test: GhosttyTerminal Storybook stories exist and render.
 *
 * Stories: Default, WithContent, ColorPalette
 */
describe("GhosttyTerminal Stories", () => {
  it("renders Default story with empty terminal", () => {
    cy.visit("/iframe.html?id=components-ghosttyterminal--default&viewMode=story");
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should(
      "exist"
    );
  });

  it("renders WithContent story with ANSI-colored output", () => {
    cy.visit(
      "/iframe.html?id=components-ghosttyterminal--with-content&viewMode=story"
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should(
      "exist"
    );
  });

  it("renders ColorPalette story", () => {
    cy.visit(
      "/iframe.html?id=components-ghosttyterminal--color-palette&viewMode=story"
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should(
      "exist"
    );
  });
});
