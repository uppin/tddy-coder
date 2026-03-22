import React from "react";
import { GhosttyTerminalLiveKit } from "../../src/components/GhosttyTerminalLiveKit";

/** VT CSI arrow sequences (must match `handleMobileKeyDown` / shared enqueue path). */
const CSI_ARROW_UP = [0x1b, 0x5b, 0x41];
const CSI_ARROW_DOWN = [0x1b, 0x5b, 0x42];
const CSI_ARROW_RIGHT = [0x1b, 0x5b, 0x43];
const CSI_ARROW_LEFT = [0x1b, 0x5b, 0x44];

/**
 * Captures `Array.from(encoded)` from `enqueueTerminalInput` (see `[terminal→server]` log in GhosttyTerminalLiveKit).
 */
function installTerminalToServerSpy(win: Window) {
  const chunks: number[][] = [];
  const orig = win.console.log;
  win.console.log = (...args: unknown[]) => {
    orig.apply(win.console, args);
    if (
      args.length >= 4 &&
      args[0] === "[terminal→server]" &&
      Array.isArray(args[3])
    ) {
      chunks.push([...(args[3] as number[])]);
    }
  };
  return {
    getArrowCsiChunks: () =>
      chunks.filter(
        (b) =>
          b.length === 3 &&
          b[0] === 0x1b &&
          b[1] === 0x5b &&
          (b[2] === 0x41 || b[2] === 0x42 || b[2] === 0x43 || b[2] === 0x44)
      ),
    restore: () => {
      win.console.log = orig;
    },
  };
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

  describe("CLI mode (acceptance)", () => {
    it("CLI mode toggle shows and hides bottom-right arrow pad", () => {
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
      cy.get("[data-testid='ghostty-cli-mode-toggle']", { timeout: 10000 }).should("exist");
      cy.get("[data-testid='ghostty-cli-arrow-up']").should("not.exist");
      cy.get("[data-testid='ghostty-cli-arrow-down']").should("not.exist");
      cy.get("[data-testid='ghostty-cli-arrow-left']").should("not.exist");
      cy.get("[data-testid='ghostty-cli-arrow-right']").should("not.exist");

      cy.get("[data-testid='ghostty-cli-mode-toggle']").click();
      cy.get("[data-testid='ghostty-cli-arrow-up']").should("be.visible");
      cy.get("[data-testid='ghostty-cli-arrow-down']").should("be.visible");
      cy.get("[data-testid='ghostty-cli-arrow-left']").should("be.visible");
      cy.get("[data-testid='ghostty-cli-arrow-right']").should("be.visible");

      cy.get("[data-testid='ghostty-cli-mode-toggle']").click();
      cy.get("[data-testid='ghostty-cli-arrow-up']").should("not.exist");
      cy.get("[data-testid='ghostty-cli-arrow-down']").should("not.exist");
      cy.get("[data-testid='ghostty-cli-arrow-left']").should("not.exist");
      cy.get("[data-testid='ghostty-cli-arrow-right']").should("not.exist");
    });

    it("arrow pad emits CSI sequences for Up, Down, Left, Right via shared enqueue path", () => {
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
      cy.get("[data-testid='ghostty-cli-mode-toggle']", { timeout: 10000 }).should("exist");
      cy.window().then((win) => {
        const spy = installTerminalToServerSpy(win);
        cy.get("[data-testid='ghostty-cli-mode-toggle']").click();
        cy.get("[data-testid='ghostty-cli-arrow-up']").click();
        cy.get("[data-testid='ghostty-cli-arrow-down']").click();
        cy.get("[data-testid='ghostty-cli-arrow-right']").click();
        cy.get("[data-testid='ghostty-cli-arrow-left']").click();
        cy.then(() => {
          expect(spy.getArrowCsiChunks()).to.deep.equal([
            CSI_ARROW_UP,
            CSI_ARROW_DOWN,
            CSI_ARROW_RIGHT,
            CSI_ARROW_LEFT,
          ]);
          spy.restore();
        });
      });
    });
  });
});
