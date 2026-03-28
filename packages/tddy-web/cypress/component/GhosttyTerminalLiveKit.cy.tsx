import React, { useState } from "react";
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

describe("GhosttyTerminalLiveKit — web terminal terminate / remote session ended (acceptance)", () => {
  const getToken = () => Promise.resolve({ token: "fake-token", ttlSeconds: BigInt(600) });

  it("GhosttyTerminalLiveKit shows Terminate next to Ctrl+C and Disconnect when overlay props include terminate action", () => {
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://127.0.0.1:59999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          connectionOverlay={{
            onDisconnect: () => {},
            onTerminate: () => {},
          }}
        />
      </div>,
    );
    cy.get("[data-testid='ctrl-c-button']", { timeout: 5000 }).should("exist");
    cy.get("[data-testid='disconnect-button']").should("exist");
    cy.get("[data-testid='terminal-overlay-terminate']")
      .should("exist")
      .and("be.visible")
      .and("contain.text", "Terminate");
  });

  it("Clicking Terminate invokes the provided SIGINT / signal callback once", () => {
    const onDisconnect = cy.stub().as("onDisconnect");
    const onTerminate = cy.stub().as("onTerminate");
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://127.0.0.1:59999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          connectionOverlay={{
            onDisconnect,
            onTerminate,
          }}
        />
      </div>,
    );
    cy.get("[data-testid='terminal-overlay-terminate']", { timeout: 5000 }).should("exist");
    cy.get("[data-testid='terminal-overlay-terminate']").click();
    cy.get("@onTerminate").should("have.been.calledOnce");
    cy.get("@onDisconnect").should("not.have.been.called");
  });

  it("When onRemoteSessionEnded (or equivalent) fires, parent can clear connected state", () => {
    const onRemoteSessionEnded = cy.stub().as("onRemoteSessionEnded");

    function Harness() {
      const [phase, setPhase] = useState<"terminal" | "connection">("terminal");
      if (phase === "connection") {
        return <div data-testid="acceptance-parent-at-connection">Connection</div>;
      }
      return (
        <div style={{ height: 400, position: "relative" }}>
          <GhosttyTerminalLiveKit
            url="ws://127.0.0.1:59999"
            token="fake-token"
            getToken={getToken}
            ttlSeconds={BigInt(600)}
            connectionOverlay={{
              onDisconnect: () => {},
            }}
            simulateRemoteDisconnectAfterMs={500}
            onRemoteSessionEnded={() => {
              onRemoteSessionEnded();
              setPhase("connection");
            }}
          />
        </div>
      );
    }

    cy.mount(<Harness />);
    cy.get("[data-testid='acceptance-parent-at-connection']").should("not.exist");
    cy.get("[data-testid='disconnect-button']", { timeout: 5000 }).should("exist");
    cy.get("[data-testid='acceptance-parent-at-connection']", { timeout: 20000 }).should("exist");
    cy.get("@onRemoteSessionEnded").should("have.been.calledOnce");
  });
});
