import React from "react";
import { Button } from "../../src/components/ui/button";

describe("Button", () => {
  it("renders with a label and fires onClick when clicked", () => {
    // Given
    const onClick = cy.stub().as("onClick");
    cy.mount(
      <Button type="button" onClick={onClick}>
        Click me
      </Button>,
    );

    // When
    cy.get("button").click();

    // Then
    cy.get("button").should("have.text", "Click me");
    cy.get("@onClick").should("have.been.calledOnce");
  });
});
