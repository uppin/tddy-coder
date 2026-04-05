import React from "react";
import {
  plannedChromeCentersClearTerminalCanvas,
  statusBarBottomMeetsOrAboveTerminalTop,
  type ViewRect,
} from "../../src/lib/terminalStatusBarLayout";
import { GhosttyTerminalLiveKit } from "../../src/components/GhosttyTerminalLiveKit";

function domRectToViewRect(r: DOMRect): ViewRect {
  return { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
}

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

  it("GhosttyTerminalLiveKit renders status dot and build id when connection overlay is enabled", () => {
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

  it("hides visible livekit status text when connection overlay is enabled", () => {
    const onDisconnect = cy.stub();
    cy.mount(
      <div data-testid="terminal-chrome-host" style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          connectionOverlay={{ onDisconnect, buildId: "acceptance-build" }}
        />
      </div>
    );
    cy.get("[data-testid='connection-status-dot']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='livekit-status']").should("not.be.visible");
    cy.get("[data-testid='connection-status-dot']").should("have.attr", "data-connection-status");
  });

  it("fullscreen toggle invokes requestFullscreen when stubbed (enter path)", () => {
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
      cy.stub(win.Element.prototype, "requestFullscreen").as("requestFullscreenStub").resolves();
    });
    cy.get("[data-testid='connection-status-dot']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='terminal-fullscreen-button']", { timeout: 5000 }).should("be.visible").click();
    cy.get("@requestFullscreenStub").should("have.been.calledOnce");
  });

  it("Terminate does not call onTerminate when confirmation is cancelled", () => {
    const onDisconnect = cy.stub();
    const onTerminate = cy.stub().as("onTerminate");
    cy.window().then((win) => {
      cy.stub(win, "confirm").returns(false);
    });
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          connectionOverlay={{ onDisconnect, onTerminate }}
        />
      </div>
    );
    cy.get("[data-testid='connection-status-dot']", { timeout: 10000 }).should("exist").click();
    cy.get("[data-testid='connection-menu-terminate']", { timeout: 3000 }).should("be.visible").click();
    cy.get("@onTerminate").should("not.have.been.called");
  });

  it("Terminate calls onTerminate once after user confirms", () => {
    const onDisconnect = cy.stub();
    const onTerminate = cy.stub().as("onTerminate");
    cy.window().then((win) => {
      cy.stub(win, "confirm").returns(true).as("confirmTerminate");
    });
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          connectionOverlay={{ onDisconnect, onTerminate }}
        />
      </div>
    );
    cy.get("[data-testid='connection-status-dot']", { timeout: 10000 }).should("exist").click();
    cy.get("[data-testid='connection-menu-terminate']", { timeout: 3000 }).should("be.visible").click();
    cy.get("@onTerminate").should("have.been.calledOnce");
    cy.get("@confirmTerminate").should("have.been.calledOnce");
    cy.get("@confirmTerminate").then((stub: unknown) => {
      const s = stub as { getCall: (n: number) => { args: string[] } };
      const msg = String(s.getCall(0).args[0] ?? "");
      expect(
        msg.toLowerCase().includes("stop") ||
          msg.toLowerCase().includes("terminat") ||
          msg.toLowerCase().includes("session") ||
          msg.toLowerCase().includes("process"),
        "confirmation copy should mention stopping the session or process",
      ).to.be.true;
    });
  });
});

/**
 * PRD Testing Plan — terminal status bar (chrome not over Ghostty canvas).
 * These tests expect `data-testid="terminal-connection-status-bar"` and layout where the bar
 * precedes `[data-testid="ghostty-terminal"]` with no control centers intersecting the terminal rect.
 */
describe("Terminal status bar acceptance (PRD)", () => {
  const getToken = () => Promise.resolve({ token: "fake-token", ttlSeconds: BigInt(600) });

  function assertControlCentersNotInsideTerminalCanvas() {
    cy.get("[data-testid='ghostty-terminal']").then(($term) => {
      const termRect = domRectToViewRect($term[0].getBoundingClientRect());
      const doc = $term[0].ownerDocument;
      const selectors = [
        "[data-testid='connection-status-dot']",
        "[data-testid='terminal-fullscreen-button']",
        "[data-testid='build-id']",
        "[data-testid='mobile-keyboard-button']",
      ];
      const controlRects: ViewRect[] = [];
      for (const sel of selectors) {
        const el = doc.querySelector(sel);
        if (!el) {
          throw new Error(`expected ${sel} to exist for geometry assertion`);
        }
        controlRects.push(domRectToViewRect(el.getBoundingClientRect()));
      }
      expect(
        plannedChromeCentersClearTerminalCanvas(termRect, controlRects),
        "control centers must not lie inside ghostty-terminal rect (shared layout helper)",
      ).to.be.true;
    });
  }

  it("ghostty_livekit_chrome_lives_in_status_bar_not_over_canvas", () => {
    const onDisconnect = cy.stub();
    cy.mount(
      <div data-testid="ghostty-livekit-acceptance-host" style={{ height: 420, width: 640, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          connectionOverlay={{ onDisconnect, buildId: "acceptance-build" }}
          showMobileKeyboard
        />
      </div>,
    );
    cy.get("[data-testid='terminal-connection-status-bar']", { timeout: 20000 }).should("exist");
    cy.get("[data-testid='terminal-connection-status-bar']").within(() => {
      cy.get("[data-testid='connection-status-dot']").should("exist");
      cy.get("[data-testid='build-id']").should("contain.text", "acceptance-build");
      cy.get("[data-testid='terminal-fullscreen-button']").should("exist");
      cy.get("[data-testid='mobile-keyboard-button']").should("exist");
    });
    cy.get("[data-testid='ghostty-terminal']", { timeout: 20000 }).should("exist");
    cy.get("[data-testid='terminal-connection-status-bar']").then(($bar) => {
      cy.get("[data-testid='ghostty-terminal']").then(($term) => {
        const bar = $bar[0];
        const term = $term[0];
        expect(
          bar.compareDocumentPosition(term) & Node.DOCUMENT_POSITION_FOLLOWING,
          "status bar must precede ghostty-terminal in document order",
        ).to.be.ok;
        expect(
          statusBarBottomMeetsOrAboveTerminalTop(
            domRectToViewRect(bar.getBoundingClientRect()),
            domRectToViewRect(term.getBoundingClientRect()),
          ),
          "status bar bottom must meet or be above terminal top",
        ).to.be.true;
      });
    });
    assertControlCentersNotInsideTerminalCanvas();
  });

  it("connection_menu_and_fullscreen_still_functional", () => {
    const onDisconnect = cy.stub().as("onDisconnect");
    cy.mount(
      <div style={{ height: 420, width: 640, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          connectionOverlay={{ onDisconnect, buildId: "menu-build" }}
        />
      </div>,
    );
    cy.window().then((win) => {
      cy.stub(win.Element.prototype, "requestFullscreen").as("requestFullscreenStub").resolves();
    });
    cy.get("[data-testid='terminal-connection-status-bar'] [data-testid='connection-status-dot']", { timeout: 20000 })
      .should("be.visible")
      .click();
    cy.get("[data-testid='connection-menu-disconnect']", { timeout: 5000 }).should("be.visible");
    cy.get("[data-testid='connection-menu-disconnect']").click();
    cy.get("@onDisconnect").should("have.been.calledOnce");
    cy.get("[data-testid='terminal-connection-status-bar'] [data-testid='terminal-fullscreen-button']", {
      timeout: 20000,
    })
      .should("be.visible")
      .click();
    cy.get("@requestFullscreenStub").should("have.been.calledOnce");
  });

  it("mobile_keyboard_affordance_in_status_bar", () => {
    cy.mount(
      <div style={{ height: 420, width: 640, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          connectionOverlay={{ onDisconnect: () => {} }}
          showMobileKeyboard
          preventFocusOnTap={false}
        />
      </div>,
    );
    cy.get("[data-testid='terminal-connection-status-bar'] [data-testid='mobile-keyboard-button']", { timeout: 20000 })
      .should("exist")
      .and("contain.text", "Keyboard");
    cy.get("[data-testid='terminal-connection-status-bar']").then(($bar) => {
      cy.get("[data-testid='ghostty-terminal']").then(($term) => {
        expect(
          statusBarBottomMeetsOrAboveTerminalTop(
            domRectToViewRect($bar[0].getBoundingClientRect()),
            domRectToViewRect($term[0].getBoundingClientRect()),
          ),
          "mobile keyboard row must sit above the terminal canvas",
        ).to.be.true;
      });
    });
  });
});
