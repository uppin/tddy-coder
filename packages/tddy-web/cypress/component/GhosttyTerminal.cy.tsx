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

  it("fires onResize when terminal dimensions change (FitAddon)", () => {
    const onResize = cy.stub().as("onResize");
    cy.mount(<GhosttyTerminal onResize={onResize} />);
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    // FitAddon triggers resize on mount; wait for it
    cy.get("@onResize", { timeout: 5000 }).should("have.been.called");
  });

  it("forwards SGR mouse sequence via onData when mouse tracking enabled and user clicks", () => {
    const onData = cy.stub().as("onData");
    const enableMouse = "\x1b[?1000h\x1b[?1006h";
    cy.mount(
      <GhosttyTerminal
        initialContent={enableMouse}
        onData={onData}
      />
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    cy.wait(500);
    cy.get("[data-testid='ghostty-terminal']").click("center");
    cy.get("@onData").should((stub) => {
      const calls = stub.getCalls();
      const sgrMouseCalls = calls.filter(
        (c: { args: unknown[] }) =>
          typeof c.args[0] === "string" && /^\x1b\[<0;\d+;\d+[Mm]$/.test(c.args[0])
      );
      expect(sgrMouseCalls.length, "onData should receive SGR mouse sequence").to.be.greaterThan(0);
    });
  });
});
