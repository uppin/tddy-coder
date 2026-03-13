import React from "react";
import { GhosttyTerminal } from "../../src/components/GhosttyTerminal";

describe("GhosttyTerminal", () => {
  it("renders ANSI content passed via initialContent prop", () => {
    const ansiContent = "\x1b[1;32m$ ls -la\x1b[0m\nfile1.txt  file2.txt";
    cy.mount(<GhosttyTerminal initialContent={ansiContent} />);
    cy.get("[data-testid='ghostty-terminal']").should("exist");
    cy.get("[data-testid='ghostty-terminal']").within(() => {
      cy.get("canvas").should("exist");
    });
  });

  it("fires onData when keyboard input is sent", () => {
    const onData = cy.stub().as("onData");
    cy.mount(<GhosttyTerminal onData={onData} />);
    cy.get("[data-testid='ghostty-terminal']").click();
    cy.get("[data-testid='ghostty-terminal']").type("x");
    cy.get("@onData").should("have.been.called");
  });
});
