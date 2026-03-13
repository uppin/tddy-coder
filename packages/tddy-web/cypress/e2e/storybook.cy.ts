describe("Storybook", () => {
  it("renders Button Primary story", () => {
    cy.visit("/iframe.html?id=components-button--primary&viewMode=story");
    cy.contains("button", "Primary", { timeout: 15000 }).should("exist");
  });
});
