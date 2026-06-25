import React from "react";
import { ShortcutDrawer } from "../../src/components/connection/ShortcutDrawer";
import { keySequenceToBytes } from "../../src/lib/toolShortcuts";
import { byTestId, TEST_IDS, shortcutButton } from "../support/testIds";

const SAMPLE_SHORTCUTS = [
  { label: "Shift+Tab", keys: ["Shift", "Tab"] },
  { label: "Ctrl+C", keys: ["Ctrl", "C"] },
  { label: "Escape", keys: ["Escape"] },
];

describe("ShortcutDrawer", () => {
  beforeEach(() => {
    cy.viewport(800, 600);
  });

  it("renders a button for each shortcut", () => {
    // Given
    cy.mount(
      <ShortcutDrawer
        shortcuts={SAMPLE_SHORTCUTS}
        onSend={cy.stub()}
        viewportHeight={600}
      />,
    );

    // Then
    byTestId(TEST_IDS.shortcutDrawer).should("exist");
    cy.get(`[data-testid='${shortcutButton("Shift+Tab")}']`).should("exist");
    cy.get(`[data-testid='${shortcutButton("Ctrl+C")}']`).should("exist");
    cy.get(`[data-testid='${shortcutButton("Escape")}']`).should("exist");
  });

  it("renders nothing when the shortcuts list is empty", () => {
    // Given
    cy.mount(
      <ShortcutDrawer shortcuts={[]} onSend={cy.stub()} viewportHeight={600} />,
    );

    // Then
    byTestId(TEST_IDS.shortcutDrawer).should("not.exist");
  });

  it("calls onSend with the correct bytes when a shortcut button is clicked", () => {
    // Given
    const onSend = cy.stub().as("onSend");
    cy.mount(
      <ShortcutDrawer
        shortcuts={SAMPLE_SHORTCUTS}
        onSend={onSend}
        viewportHeight={600}
      />,
    );

    // When
    cy.get(`[data-testid='${shortcutButton("Shift+Tab")}']`).click({ force: true });

    // Then — onSend receives the Shift+Tab escape sequence \x1b[Z
    cy.get("@onSend").should("have.been.calledOnce");
    cy.get("@onSend").then((stub) => {
      const arg: Uint8Array = (stub as sinon.SinonStub).getCall(0).args[0] as Uint8Array;
      const expected = keySequenceToBytes(["Shift", "Tab"]);
      expect(Array.from(arg)).to.deep.equal(Array.from(expected));
    });
  });

  it("calls onSend with the Ctrl+C byte when the Ctrl+C button is clicked", () => {
    // Given
    const onSend = cy.stub().as("onSend");
    cy.mount(
      <ShortcutDrawer
        shortcuts={SAMPLE_SHORTCUTS}
        onSend={onSend}
        viewportHeight={600}
      />,
    );

    // When
    cy.get(`[data-testid='${shortcutButton("Ctrl+C")}']`).click({ force: true });

    // Then — Ctrl+C = 0x03
    cy.get("@onSend").should("have.been.calledOnce");
    cy.get("@onSend").then((stub) => {
      const arg: Uint8Array = (stub as sinon.SinonStub).getCall(0).args[0] as Uint8Array;
      expect(Array.from(arg)).to.deep.equal([0x03]);
    });
  });

  it("shows the drag handle", () => {
    // Given
    cy.mount(
      <ShortcutDrawer
        shortcuts={SAMPLE_SHORTCUTS}
        onSend={cy.stub()}
        viewportHeight={600}
      />,
    );

    // Then
    byTestId(TEST_IDS.shortcutDragHandle).should("exist");
  });

  it("snaps to the right edge after dragging rightward past the viewport center", () => {
    // Given — drawer starts at bottom snap
    cy.mount(
      <div style={{ width: "100vw", height: "100vh" }}>
        <ShortcutDrawer
          shortcuts={SAMPLE_SHORTCUTS}
          onSend={cy.stub()}
          viewportHeight={600}
        />
      </div>,
    );
    byTestId(TEST_IDS.shortcutDrawer).should("have.attr", "data-snap-edge", "bottom");

    // When — drag the handle toward the right edge
    byTestId(TEST_IDS.shortcutDragHandle)
      .trigger("pointerdown", { clientX: 100, clientY: 560, pointerId: 1, force: true })
      .trigger("pointermove", { clientX: 700, clientY: 300, pointerId: 1, force: true })
      .trigger("pointerup", { clientX: 700, clientY: 300, pointerId: 1, force: true });

    // Then — drawer snapped to the right edge
    byTestId(TEST_IDS.shortcutDrawer).should("have.attr", "data-snap-edge", "right");
  });

  it("lays out buttons in a row when snapped to the bottom edge", () => {
    // Given
    cy.mount(
      <ShortcutDrawer
        shortcuts={SAMPLE_SHORTCUTS}
        onSend={cy.stub()}
        viewportHeight={600}
      />,
    );

    // Then — default snap is bottom; buttons should share the same top coordinate (row layout)
    cy.get(`[data-testid='${shortcutButton("Shift+Tab")}']`).then(($first) => {
      cy.get(`[data-testid='${shortcutButton("Ctrl+C")}']`).then(($second) => {
        const firstRect = $first[0].getBoundingClientRect();
        const secondRect = $second[0].getBoundingClientRect();
        // In a row layout, tops are equal (±4px rounding)
        expect(Math.abs(firstRect.top - secondRect.top)).to.be.lessThan(4);
        // And second button is to the right of the first
        expect(secondRect.left).to.be.greaterThan(firstRect.right - 1);
      });
    });
  });
});
