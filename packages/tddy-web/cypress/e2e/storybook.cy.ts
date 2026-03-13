describe("Storybook", () => {
  it("renders Button Primary story", () => {
    cy.visit("/iframe.html?id=button--primary");
    cy.get("button").should("be.visible").and("have.text", "Primary");
  });
});
