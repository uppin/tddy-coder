import React from "react";
import { Button } from "../../src/components/ui/button";

describe("Button", () => {
  it("renders with label and fires onClick", () => {
    const onClick = cy.stub().as("onClick");
    cy.mount(
      <Button type="button" onClick={onClick}>
        Click me
      </Button>,
    );
    cy.get("button").should("have.text", "Click me");
    cy.get("button").click();
    cy.get("@onClick").should("have.been.calledOnce");
  });
});
