import React from "react";
import { ConnectionTerminalChrome } from "../../src/components/connection/ConnectionTerminalChrome";

describe("ConnectionTerminalChrome", () => {
  it("places fullscreen control to the right of the connection status dot", () => {
    const onDisconnect = cy.stub();
    const onStopInterrupt = cy.stub();
    cy.mount(
      <div style={{ position: "relative", width: 480, height: 320 }}>
        <ConnectionTerminalChrome
          overlayStatus="connected"
          onDisconnect={onDisconnect}
          onStopInterrupt={onStopInterrupt}
        />
      </div>
    );
    cy.get("[data-testid='connection-status-dot']").should("exist");
    cy.get("[data-testid='terminal-fullscreen-button']").should("exist");
    cy.window().then((win) => {
      cy.stub(win.Element.prototype, "requestFullscreen").as("requestFullscreenStub").resolves();
    });
    cy.get("[data-testid='terminal-fullscreen-button']").click();
    cy.get("@requestFullscreenStub").should("have.been.calledOnce");
    cy.get("[data-testid='connection-status-dot']").then(($dot) => {
      cy.get("[data-testid='terminal-fullscreen-button']").then(($btn) => {
        const dot = $dot[0].getBoundingClientRect();
        const btn = $btn[0].getBoundingClientRect();
        expect(btn.left, "fullscreen control should sit to the right of the dot").to.be.greaterThan(dot.right);
      });
    });
  });
});
