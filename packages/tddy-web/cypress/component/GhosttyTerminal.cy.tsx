import React, { useRef } from "react";
import { aGhosttyTerminal } from "../support/drivers/ghosttyTerminalDriver";

describe("GhosttyTerminal", () => {
  it("renders ANSI content passed via initialContent prop", () => {
    // Given
    const ansiContent = "\x1b[1;32m$ ls -la\x1b[0m\nfile1.txt  file2.txt";

    // When / Then
    aGhosttyTerminal({ initialContent: ansiContent }).mount().expectExists().expectCanvasExists();
  });

  it("fires onData when keyboard input is sent", () => {
    // Given
    const driver = aGhosttyTerminal({ onData: cy.stub().as("onData") }).mount();

    // When
    driver.click();
    driver.type("x");

    // Then
    driver.expectOnDataCalled();
  });

  it("forwards Escape to onData during IME composition (ghostty-web would otherwise drop it)", () => {
    // Given
    const driver = aGhosttyTerminal({ onData: cy.stub().as("onData"), sessionActive: true }).mount();
    driver.expectExists();
    driver.el().find("textarea").should("exist");

    // Flush useEffect that registers the capture listener (runs after paint)
    cy.wrap(null).then(
      () =>
        new Promise<void>((resolve) => {
          requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
        }),
    );

    // When — dispatch a composing Escape keydown directly on the textarea
    driver.el().find("textarea").then(($ta) => {
      const ta = $ta[0];
      const ev = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
      Object.defineProperty(ev, "isComposing", { configurable: true, get: () => true });
      Object.defineProperty(ev, "keyCode", { value: 229, configurable: true });
      ta.dispatchEvent(ev);
    });

    // Then
    cy.get("@onData").should("have.been.calledWith", "\x1b");
  });

  it("fires onResize when terminal dimensions change (FitAddon)", () => {
    // Given / When
    const driver = aGhosttyTerminal({ onResize: cy.stub().as("onResize") }).mount();
    driver.expectExists();

    // Then — FitAddon triggers resize on mount
    driver.expectOnResizeCalled(5000);
  });

  it("forwards SGR mouse sequence via onData when mouse tracking is enabled and the user touches the terminal", () => {
    // Given
    const driver = aGhosttyTerminal({
      onData: cy.stub().as("onData"),
      initialContent: "\x1b[?1000h\x1b[?1006h",
    }).mount();
    driver.expectExists();

    // Brief settle — mouse-tracking escape sequences need a render cycle before events work
    // eslint-disable-next-line cypress/no-unnecessary-waiting
    cy.wait(500); // justified: ghostty-web must process the SGR enable sequences before we send touch events

    // When
    driver.simulateTouchTap();

    // Then
    cy.get("@onData").should((subject) => {
      const stub = subject as unknown as { getCalls: () => { args: unknown[] }[] };
      const sgrCalls = stub.getCalls().filter(
        (c) => typeof c.args[0] === "string" && /^\x1b\[<0;\d+;\d+[Mm]$/.test(c.args[0] as string),
      );
      expect(sgrCalls.length, "onData should receive SGR mouse sequence from touch").to.be.greaterThan(0);
    });
  });

  it("does not receive focus when clicked with preventFocusOnTap true", () => {
    // Given / When
    const driver = aGhosttyTerminal({ preventFocusOnTap: true }).mount();
    driver.expectExists().expectCanvasExists();
    driver.click("center");

    // Then
    driver.expectNoFocus("terminal should not receive focus when preventFocusOnTap is true");
  });

  it("does not receive focus when touched with preventFocusOnTap true (mobile tap)", () => {
    // Given
    const driver = aGhosttyTerminal({ preventFocusOnTap: true }).mount();
    driver.expectExists().expectCanvasExists();

    // When
    driver.simulateTouchTap();
    // eslint-disable-next-line cypress/no-unnecessary-waiting
    cy.wait(50); // justified: touch event processing is async; give React time to commit

    // Then
    driver.expectNoFocus("terminal should not receive focus when touched with preventFocusOnTap true");
  });

  it("does not receive focus when a click event fires (mobile keyboard-opens-on-tap flow)", () => {
    // Given
    const driver = aGhosttyTerminal({ preventFocusOnTap: true }).mount();
    driver.expectExists();

    // When
    driver.el().then(($el) => {
      const el = $el[0];
      const rect = el.getBoundingClientRect();
      el.dispatchEvent(
        new MouseEvent("click", {
          bubbles: true,
          cancelable: true,
          view: window,
          clientX: rect.left + rect.width / 2,
          clientY: rect.top + rect.height / 2,
        }),
      );
    });
    // eslint-disable-next-line cypress/no-unnecessary-waiting
    cy.wait(50); // justified: event processing is async

    // Then
    driver.expectNoFocus("terminal should not receive focus when click fires with preventFocusOnTap");
  });

  it("receives keyboard input after focus() when preventFocusOnTap (mobile keyboard flow)", () => {
    // Given
    const driver = aGhosttyTerminal({
      withMobileKeyboardWrapper: true,
      onData: cy.stub().as("onData"),
    }).mount();
    driver.expectExists();

    // When — open mobile keyboard and type
    driver.focusViaKeyboardButton().type("x");

    // Then
    driver.expectOnDataCalledWith("x");
  });

  it("forwards SGR mouse sequence via onData when mouse tracking is enabled and the user clicks", () => {
    // Given
    const driver = aGhosttyTerminal({
      onData: cy.stub().as("onData"),
      initialContent: "\x1b[?1000h\x1b[?1006h",
    }).mount();
    driver.expectExists();

    // Brief settle — see note above about SGR enable sequences
    // eslint-disable-next-line cypress/no-unnecessary-waiting
    cy.wait(500); // justified: ghostty-web must process SGR enable sequences before mouse events work

    // When
    driver.click("center");

    // Then
    cy.get("@onData").should((subject) => {
      const stub = subject as unknown as { getCalls: () => { args: unknown[] }[] };
      const sgrCalls = stub.getCalls().filter(
        (c) => typeof c.args[0] === "string" && /^\x1b\[<0;\d+;\d+[Mm]$/.test(c.args[0] as string),
      );
      expect(sgrCalls.length, "onData should receive SGR mouse sequence").to.be.greaterThan(0);
    });
  });
});
