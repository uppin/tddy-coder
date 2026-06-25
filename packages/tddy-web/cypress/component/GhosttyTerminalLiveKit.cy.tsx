import React from "react";
import {
  plannedChromeCentersClearTerminalCanvas,
  statusBarBottomMeetsOrAboveTerminalTop,
  type ViewRect,
} from "../../src/lib/terminalStatusBarLayout";
import { aGhosttyTerminalLiveKit } from "../support/drivers/ghosttyTerminalLiveKitDriver";

// ---------------------------------------------------------------------------
// Geometry helpers — kept file-local because they involve complex layout logic
// ---------------------------------------------------------------------------

function domRectToViewRect(r: DOMRect): ViewRect {
  return { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
}

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

// ---------------------------------------------------------------------------
// Main describe block
// ---------------------------------------------------------------------------

describe("GhosttyTerminalLiveKit", () => {
  it("shows mobile keyboard overlay when showMobileKeyboard is true regardless of preventFocusOnTap", () => {
    // Given / When
    const driver = aGhosttyTerminalLiveKit({ showMobileKeyboard: true, preventFocusOnTap: false }).mount();

    // Then
    driver.mobileKeyboardButton().should("exist").and("contain.text", "Keyboard");
  });

  it("mobile keyboard overlay input forwards typed characters when focused", () => {
    // Given
    const driver = aGhosttyTerminalLiveKit({ showMobileKeyboard: true, preventFocusOnTap: false }).mount();
    driver.mobileKeyboardButton().should("exist");

    // When
    driver.mobileKeyboardButton().within(() => {
      cy.get("input").focus();
    });
    driver.mobileKeyboardButton().within(() => {
      cy.get("input").type("x");
    });

    // Then
    driver.mobileKeyboardButton().within(() => {
      cy.get("input").should("have.value", "");
    });
  });

  it("does not show mobile keyboard overlay when showMobileKeyboard is false", () => {
    // Given / When
    const driver = aGhosttyTerminalLiveKit({ showMobileKeyboard: false }).mount();

    // Then
    driver.expectMobileKeyboardNotExists();
  });

  it("GhosttyTerminalLiveKit renders status dot and build id when connection overlay is enabled", () => {
    // Given / When
    const driver = aGhosttyTerminalLiveKit({
      connectionOverlay: { onDisconnect: cy.stub().as("onDisconnect"), buildId: "test-build" },
    }).mount();

    // Then
    driver.statusDot().should("exist").and("have.attr", "data-connection-status");
    driver.buildId().should("contain.text", "test-build");
    cy.get("[data-testid='ctrl-c-button']").should("not.exist");
  });

  it("Opening the status menu shows Disconnect and conditionally Terminate", () => {
    // Given
    const driver = aGhosttyTerminalLiveKit({
      connectionOverlay: { onDisconnect: cy.stub().as("onDisconnect") },
    }).mount();

    // When
    driver.openStatusMenu();

    // Then
    driver.disconnectMenuItem().should("be.visible");
    // Without session terminate wiring, Terminate must not be a silent no-op: omit from menu.
    driver.terminateMenuItem().should("not.exist");

    // When — click disconnect
    driver.clickDisconnect();

    // Then
    driver.expectDisconnectCalled();
  });

  it("hides visible livekit status text when connection overlay is enabled", () => {
    // Given / When
    const driver = aGhosttyTerminalLiveKit({
      connectionOverlay: { onDisconnect: cy.stub(), buildId: "acceptance-build" },
    }).mount();

    // Then
    driver.statusDot().should("exist");
    driver.livekitStatus().should("not.be.visible");
    driver.statusDot().should("have.attr", "data-connection-status");
  });

  it("fullscreen toggle invokes requestFullscreen when stubbed (enter path)", () => {
    // Given
    const driver = aGhosttyTerminalLiveKit({
      connectionOverlay: { onDisconnect: cy.stub() },
    }).mount();

    // Stub must be set up after mount so the window object is ready
    driver.stubRequestFullscreen();

    // When
    driver.statusDot().should("exist");
    driver.clickFullscreen();

    // Then
    driver.expectRequestFullscreenCalled();
  });

  it("Terminate does not call onTerminate when confirmation is cancelled", () => {
    // Given
    const driver = aGhosttyTerminalLiveKit();
    driver.stubConfirm(false);
    driver.withTerminate().mount();

    // When
    driver.openStatusMenu();
    driver.clickTerminate();

    // Then
    driver.expectTerminateNotCalled();
  });

  it("Terminate calls onTerminate once after user confirms", () => {
    // Given
    const driver = aGhosttyTerminalLiveKit();
    driver.stubConfirm(true);
    driver.withTerminate().mount();

    // When
    driver.openStatusMenu();
    driver.clickTerminate();

    // Then
    driver.expectTerminateCalled();
    cy.get("@confirmStub").should("have.been.calledOnce");
    cy.get("@confirmStub").then((stub: unknown) => {
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

// ---------------------------------------------------------------------------
// ShortcutDrawer integration
// ---------------------------------------------------------------------------

describe("GhosttyTerminalLiveKit — ShortcutDrawer integration", () => {
  it("renders the shortcut drawer when mobileShortcuts are provided and showMobileKeyboard is true", () => {
    // Given
    const driver = aGhosttyTerminalLiveKit({
      showMobileKeyboard: true,
      mobileShortcuts: [
        { label: "Shift+Tab", keys: ["Shift", "Tab"] },
        { label: "Ctrl+C", keys: ["Ctrl", "C"] },
      ],
    }).mount();

    // Then
    driver.expectShortcutDrawerExists();
  });

  it("does not render the shortcut drawer when mobileShortcuts is empty", () => {
    // Given
    const driver = aGhosttyTerminalLiveKit({
      showMobileKeyboard: true,
      mobileShortcuts: [],
    }).mount();

    // Then
    driver.expectShortcutDrawerNotExists();
  });

  it("does not render the shortcut drawer when showMobileKeyboard is false even with shortcuts provided", () => {
    // Given
    const driver = aGhosttyTerminalLiveKit({
      showMobileKeyboard: false,
      mobileShortcuts: [{ label: "Shift+Tab", keys: ["Shift", "Tab"] }],
    }).mount();

    // Then
    driver.expectShortcutDrawerNotExists();
  });
});

// ---------------------------------------------------------------------------
// Terminal status bar acceptance (PRD)
// ---------------------------------------------------------------------------

/**
 * PRD Testing Plan — terminal status bar (chrome not over Ghostty canvas).
 * These tests expect `data-testid="terminal-connection-status-bar"` and layout where the bar
 * precedes `[data-testid="ghostty-terminal"]` with no control centers intersecting the terminal rect.
 */
describe("Terminal status bar acceptance (PRD)", () => {
  it("ghostty_livekit_chrome_lives_in_status_bar_not_over_canvas", () => {
    // Given
    const driver = aGhosttyTerminalLiveKit({
      connectionOverlay: { onDisconnect: cy.stub(), buildId: "acceptance-build" },
      showMobileKeyboard: true,
      containerHeight: 420,
      containerWidth: 640,
    }).mount();

    // When / Then — status bar exists and contains all chrome elements
    driver.statusBar().should("exist");
    driver.statusBar().within(() => {
      cy.get("[data-testid='connection-status-dot']").should("exist");
      cy.get("[data-testid='build-id']").should("contain.text", "acceptance-build");
      cy.get("[data-testid='terminal-fullscreen-button']").should("exist");
      cy.get("[data-testid='mobile-keyboard-button']").should("exist");
    });

    // Then — terminal exists and status bar precedes it in document order
    driver.terminal().should("exist");
    driver.statusBar().then(($bar) => {
      driver.terminal().then(($term) => {
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

    // Then — no control centers overlap the terminal canvas
    assertControlCentersNotInsideTerminalCanvas();
  });

  it("connection_menu_and_fullscreen_still_functional", () => {
    // Given
    const driver = aGhosttyTerminalLiveKit({
      connectionOverlay: { onDisconnect: cy.stub().as("onDisconnect"), buildId: "menu-build" },
      containerHeight: 420,
      containerWidth: 640,
    }).mount();

    // Stub fullscreen before interacting
    driver.stubRequestFullscreen();

    // When — open menu and disconnect
    cy.get("[data-testid='terminal-connection-status-bar'] [data-testid='connection-status-dot']", { timeout: 20000 })
      .should("be.visible")
      .click();
    driver.disconnectMenuItem().should("be.visible").click();

    // Then
    driver.expectDisconnectCalled();

    // When — click fullscreen
    cy.get("[data-testid='terminal-connection-status-bar'] [data-testid='terminal-fullscreen-button']", {
      timeout: 20000,
    })
      .should("be.visible")
      .click();

    // Then
    driver.expectRequestFullscreenCalled();
  });

  it("mobile_keyboard_affordance_in_status_bar", () => {
    // Given / When
    const driver = aGhosttyTerminalLiveKit({
      connectionOverlay: { onDisconnect: cy.stub() },
      showMobileKeyboard: true,
      preventFocusOnTap: false,
      containerHeight: 420,
      containerWidth: 640,
    }).mount();

    // Then — mobile keyboard button is in the status bar
    cy.get("[data-testid='terminal-connection-status-bar'] [data-testid='mobile-keyboard-button']", { timeout: 20000 })
      .should("exist")
      .and("contain.text", "Keyboard");

    // Then — status bar sits above the terminal canvas
    driver.statusBar().then(($bar) => {
      driver.terminal().then(($term) => {
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
