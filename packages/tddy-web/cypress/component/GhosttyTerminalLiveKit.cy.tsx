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

  it("GhosttyTerminalLiveKit renders status dot and Stop when connection overlay is enabled", () => {
    const onDisconnect = cy.stub().as("onDisconnect");
    cy.mount(
      <div data-testid="terminal-chrome-host" style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          connectionOverlay={{ onDisconnect, buildId: "test-build" }}
        />
      </div>
    );
    cy.get("[data-testid='connection-status-dot']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='connection-status-dot']").should("have.attr", "data-connection-status");
    cy.get("[data-testid='terminal-stop-button']").should("exist");
    cy.get("[data-testid='build-id']").should("contain.text", "test-build");
    cy.get("[data-testid='ctrl-c-button']").should("not.exist");
  });

  it("Opening the status menu shows Disconnect and conditionally Terminate", () => {
    const onDisconnect = cy.stub().as("onDisconnect");
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          connectionOverlay={{ onDisconnect }}
        />
      </div>
    );
    cy.get("[data-testid='connection-status-dot']", { timeout: 10000 }).should("exist").click();
    cy.get("[data-testid='connection-menu-disconnect']", { timeout: 3000 }).should("be.visible");
    // Without session terminate wiring, Terminate must not be a silent no-op: omit from menu.
    cy.get("[data-testid='connection-menu-terminate']").should("not.exist");
    cy.get("[data-testid='connection-menu-disconnect']").click();
    cy.get("@onDisconnect").should("have.been.calledOnce");
  });

  it("Stop button sends the same interrupt byte as legacy Ctrl+C", () => {
    const onDisconnect = cy.stub();
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          connectionOverlay={{ onDisconnect }}
        />
      </div>
    );
    cy.window().then((win) => {
      cy.spy(win.console, "log").as("terminalInputLog");
    });
    cy.get("[data-testid='terminal-stop-button']", { timeout: 10000 }).should("exist").click();
    cy.get("@terminalInputLog").should((spy: unknown) => {
      const s = spy as { getCalls?: () => Array<{ args: unknown[] }> };
      const calls = s.getCalls?.() ?? [];
      const has03 = calls.some((c) =>
        c.args.some((a) => Array.isArray(a) && (a as number[]).length === 1 && (a as number[])[0] === 3),
      );
      expect(has03, "enqueue path logs raw byte 0x03 like Ctrl+C").to.be.true;
    });
  });
});
