import React from "react";
import { Button } from "../../src/components/Button";

describe("Button", () => {
  it("renders with label and fires onClick", () => {
    const onClick = cy.stub().as("onClick");
    cy.mount(<Button label="Click me" onClick={onClick} />);
    cy.get("button").should("have.text", "Click me");
    cy.get("button").click();
    cy.get("@onClick").should("have.been.calledOnce");
  });
});
