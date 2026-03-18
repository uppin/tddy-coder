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

  it("forwards SGR mouse sequence via onData when touch events (touchstart/touchend) and mouse tracking enabled", () => {
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
    cy.get("[data-testid='ghostty-terminal']").then(($el) => {
      const el = $el[0];
      const rect = el.getBoundingClientRect();
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;
      if (typeof Touch === "undefined") {
        throw new Error("Touch constructor not available in this browser");
      }
      const touch = new Touch({
        identifier: 1,
        target: el,
        clientX: centerX,
        clientY: centerY,
        radiusX: 0,
        radiusY: 0,
        rotationAngle: 0,
        force: 1,
      });
      const touchStart = new TouchEvent("touchstart", {
        touches: [touch],
        targetTouches: [touch],
        changedTouches: [touch],
        cancelable: true,
      });
      const touchEnd = new TouchEvent("touchend", {
        touches: [],
        targetTouches: [],
        changedTouches: [touch],
        cancelable: true,
      });
      el.dispatchEvent(touchStart);
      el.dispatchEvent(touchEnd);
    });
    cy.get("@onData").should((subject) => {
      const stub = subject as unknown as { getCalls: () => { args: unknown[] }[] };
      const calls = stub.getCalls();
      const sgrMouseCalls = calls.filter(
        (c: { args: unknown[] }) =>
          typeof c.args[0] === "string" && /^\x1b\[<0;\d+;\d+[Mm]$/.test(c.args[0])
      );
      expect(sgrMouseCalls.length, "onData should receive SGR mouse sequence from touch").to.be.greaterThan(0);
    });
  });

  it("does not receive focus when clicked with preventFocusOnTap true", () => {
    cy.mount(<GhosttyTerminal preventFocusOnTap />);
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='ghostty-terminal']").within(() => {
      cy.get("canvas").should("exist");
    });
    cy.get("[data-testid='ghostty-terminal']").click("center");
    cy.document().then((doc) => {
      const active = doc.activeElement;
      const terminal = doc.querySelector("[data-testid='ghostty-terminal']");
      expect(
        terminal && active && terminal.contains(active),
        "terminal should not receive focus when preventFocusOnTap is true"
      ).to.be.false;
    });
  });

  it("does not receive focus when touched with preventFocusOnTap true (mobile tap)", () => {
    cy.mount(<GhosttyTerminal preventFocusOnTap />);
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='ghostty-terminal']").within(() => {
      cy.get("canvas").should("exist");
    });
    cy.get("[data-testid='ghostty-terminal']").then(($el) => {
      const el = $el[0];
      const rect = el.getBoundingClientRect();
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;
      if (typeof Touch === "undefined") {
        throw new Error("Touch constructor not available");
      }
      const touch = new Touch({
        identifier: 1,
        target: el,
        clientX: centerX,
        clientY: centerY,
        radiusX: 0,
        radiusY: 0,
        rotationAngle: 0,
        force: 1,
      });
      el.dispatchEvent(
        new TouchEvent("touchstart", {
          touches: [touch],
          targetTouches: [touch],
          changedTouches: [touch],
          cancelable: true,
        })
      );
      el.dispatchEvent(
        new TouchEvent("touchend", {
          touches: [],
          targetTouches: [],
          changedTouches: [touch],
          cancelable: true,
        })
      );
    });
    cy.wait(50);
    cy.document().then((doc) => {
      const active = doc.activeElement;
      const terminal = doc.querySelector("[data-testid='ghostty-terminal']");
      expect(
        terminal && active && terminal.contains(active),
        "terminal should not receive focus when touched with preventFocusOnTap true"
      ).to.be.false;
    });
  });

  it("does not receive focus when click fires (mobile keyboard opens on tap)", () => {
    cy.mount(<GhosttyTerminal preventFocusOnTap />);
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='ghostty-terminal']").then(($el) => {
      const el = $el[0];
      const rect = el.getBoundingClientRect();
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;
      el.dispatchEvent(
        new MouseEvent("click", {
          bubbles: true,
          cancelable: true,
          view: window,
          clientX: centerX,
          clientY: centerY,
        })
      );
    });
    cy.wait(50);
    cy.document().then((doc) => {
      const active = doc.activeElement;
      const terminal = doc.querySelector("[data-testid='ghostty-terminal']");
      expect(
        terminal && active && terminal.contains(active),
        "terminal should not receive focus when click fires (mobile keyboard opens on tap)"
      ).to.be.false;
    });
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
    cy.get("@onData").should((subject) => {
      const stub = subject as unknown as { getCalls: () => { args: unknown[] }[] };
      const calls = stub.getCalls();
      const sgrMouseCalls = calls.filter(
        (c: { args: unknown[] }) =>
          typeof c.args[0] === "string" && /^\x1b\[<0;\d+;\d+[Mm]$/.test(c.args[0])
      );
      expect(sgrMouseCalls.length, "onData should receive SGR mouse sequence").to.be.greaterThan(0);
    });
  });
});
