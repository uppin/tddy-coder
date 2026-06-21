describe("Storybook", () => {
  it("renders the Button Primary story", () => {
    // Given / When
    cy.visit("/iframe.html?id=components-button--primary&viewMode=story");

    // Then
    cy.contains("button", "Primary", { timeout: 15000 }).should("exist");
  });
});
