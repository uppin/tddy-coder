import React from "react";
import { GhosttyTerminalLiveKit } from "../../src/components/GhosttyTerminalLiveKit";

describe("GhosttyTerminalLiveKit", () => {
  const getToken = () => Promise.resolve({ token: "fake-token", ttlSeconds: BigInt(600) });

  it("shows mobile keyboard overlay when showMobileKeyboard is true regardless of preventFocusOnTap", () => {
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          showMobileKeyboard
          preventFocusOnTap={false}
        />
      </div>
    );
    cy.get("[data-testid='mobile-keyboard-button']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='mobile-keyboard-button']").should("contain.text", "Keyboard");
  });

  it("mobile keyboard overlay input forwards typed characters when focused", () => {
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          showMobileKeyboard
          preventFocusOnTap={false}
        />
      </div>
    );
    cy.get("[data-testid='mobile-keyboard-button']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='mobile-keyboard-button']").within(() => {
      cy.get("input").focus();
    });
    cy.get("[data-testid='mobile-keyboard-button']").within(() => {
      cy.get("input").type("x");
    });
    cy.get("[data-testid='mobile-keyboard-button']").within(() => {
      cy.get("input").should("have.value", "");
    });
  });

  it("does not show mobile keyboard overlay when showMobileKeyboard is false", () => {
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          showMobileKeyboard={false}
        />
      </div>
    );
    cy.get("[data-testid='mobile-keyboard-button']").should("not.exist");
  });
});
