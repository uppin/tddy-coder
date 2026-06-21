import React from "react";
import { ConnectionTerminalChrome } from "../../src/components/connection/ConnectionTerminalChrome";
import { byTestId, TEST_IDS } from "../support/testIds";

describe("ConnectionTerminalChrome", () => {
  it("places the fullscreen control to the right of the connection status dot", () => {
    // Given
    const onDisconnect = cy.stub();
    cy.mount(
      <div style={{ position: "relative", width: 480, height: 320 }}>
        <ConnectionTerminalChrome overlayStatus="connected" onDisconnect={onDisconnect} />
      </div>,
    );
    cy.window().then((win) => {
      cy.stub(win.Element.prototype, "requestFullscreen").as("requestFullscreenStub").resolves();
    });

    // When
    byTestId(TEST_IDS.terminalFullscreenButton).click();

    // Then — fullscreen API was invoked
    cy.get("@requestFullscreenStub").should("have.been.calledOnce");

    // Then — fullscreen control sits to the right of the status dot
    byTestId(TEST_IDS.connectionStatusDot).then(($dot) => {
      byTestId(TEST_IDS.terminalFullscreenButton).then(($btn) => {
        const dot = $dot[0].getBoundingClientRect();
        const btn = $btn[0].getBoundingClientRect();
        expect(btn.left, "fullscreen control should sit to the right of the dot").to.be.greaterThan(
          dot.right,
        );
      });
    });
  });
});
