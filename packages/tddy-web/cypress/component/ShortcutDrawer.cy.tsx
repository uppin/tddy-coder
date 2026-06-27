import React from "react";
import { ShortcutDrawer } from "../../src/components/connection/ShortcutDrawer";
import { keySequenceToBytes } from "../../src/lib/toolShortcuts";
import { byTestId, TEST_IDS, shortcutButton } from "../support/testIds";

const SAMPLE_SHORTCUTS = [
  { label: "Shift+Tab", keys: ["Shift", "Tab"] },
  { label: "Ctrl+C", keys: ["Ctrl", "C"] },
  { label: "Escape", keys: ["Escape"] },
];

/** Tap the control (pointer down + up with no movement) to toggle collapsed/expanded. */
function tapShortcutControl() {
  byTestId(TEST_IDS.shortcutDragHandle)
    .trigger("pointerdown", { clientX: 400, clientY: 560, pointerId: 1, force: true })
    .trigger("pointerup", { clientX: 400, clientY: 560, pointerId: 1, force: true });
}

describe("ShortcutDrawer", () => {
  beforeEach(() => {
    cy.viewport(800, 600);
  });

  it("is collapsed by default, showing only the control and no shortcut buttons", () => {
    // Given
    cy.mount(
      <ShortcutDrawer shortcuts={SAMPLE_SHORTCUTS} onSend={cy.stub()} viewportHeight={600} />,
    );

    // Then — the control is visible but the shortcut buttons are hidden
    byTestId(TEST_IDS.shortcutDrawer).should("have.attr", "data-collapsed", "true");
    byTestId(TEST_IDS.shortcutDragHandle).should("exist");
    cy.get(`[data-testid='${shortcutButton("Shift+Tab")}']`).should("not.exist");
  });

  it("expands to show a button for each shortcut when the control is tapped", () => {
    // Given — a collapsed drawer
    cy.mount(
      <ShortcutDrawer shortcuts={SAMPLE_SHORTCUTS} onSend={cy.stub()} viewportHeight={600} />,
    );

    // When — the user taps the control
    tapShortcutControl();

    // Then — every shortcut button is shown
    byTestId(TEST_IDS.shortcutDrawer).should("have.attr", "data-collapsed", "false");
    cy.get(`[data-testid='${shortcutButton("Shift+Tab")}']`).should("exist");
    cy.get(`[data-testid='${shortcutButton("Ctrl+C")}']`).should("exist");
    cy.get(`[data-testid='${shortcutButton("Escape")}']`).should("exist");
  });

  it("collapses again when the control is tapped while expanded", () => {
    // Given — an expanded drawer
    cy.mount(
      <ShortcutDrawer shortcuts={SAMPLE_SHORTCUTS} onSend={cy.stub()} viewportHeight={600} />,
    );
    tapShortcutControl();
    byTestId(TEST_IDS.shortcutDrawer).should("have.attr", "data-collapsed", "false");

    // When — the control is tapped again
    tapShortcutControl();

    // Then — it collapses and hides the buttons
    byTestId(TEST_IDS.shortcutDrawer).should("have.attr", "data-collapsed", "true");
    cy.get(`[data-testid='${shortcutButton("Shift+Tab")}']`).should("not.exist");
  });

  it("renders nothing when the shortcuts list is empty", () => {
    // Given
    cy.mount(<ShortcutDrawer shortcuts={[]} onSend={cy.stub()} viewportHeight={600} />);

    // Then
    byTestId(TEST_IDS.shortcutDrawer).should("not.exist");
  });

  it("calls onSend with the correct bytes when a shortcut button is clicked", () => {
    // Given — an expanded drawer
    const onSend = cy.stub().as("onSend");
    cy.mount(
      <ShortcutDrawer shortcuts={SAMPLE_SHORTCUTS} onSend={onSend} viewportHeight={600} />,
    );
    tapShortcutControl();

    // When
    cy.get(`[data-testid='${shortcutButton("Shift+Tab")}']`).click({ force: true });

    // Then — onSend receives the Shift+Tab escape sequence \x1b[Z
    cy.get("@onSend").should("have.been.calledOnce");
    cy.get("@onSend").then((stub) => {
      const arg: Uint8Array = (stub as unknown as sinon.SinonStub).getCall(0).args[0] as Uint8Array;
      const expected = keySequenceToBytes(["Shift", "Tab"]);
      expect(Array.from(arg)).to.deep.equal(Array.from(expected));
    });
  });

  it("calls onSend with the Ctrl+C byte when the Ctrl+C button is clicked", () => {
    // Given — an expanded drawer
    const onSend = cy.stub().as("onSend");
    cy.mount(
      <ShortcutDrawer shortcuts={SAMPLE_SHORTCUTS} onSend={onSend} viewportHeight={600} />,
    );
    tapShortcutControl();

    // When
    cy.get(`[data-testid='${shortcutButton("Ctrl+C")}']`).click({ force: true });

    // Then — Ctrl+C = 0x03
    cy.get("@onSend").should("have.been.calledOnce");
    cy.get("@onSend").then((stub) => {
      const arg: Uint8Array = (stub as unknown as sinon.SinonStub).getCall(0).args[0] as Uint8Array;
      expect(Array.from(arg)).to.deep.equal([0x03]);
    });
  });

  it("shows the drag handle", () => {
    // Given
    cy.mount(
      <ShortcutDrawer shortcuts={SAMPLE_SHORTCUTS} onSend={cy.stub()} viewportHeight={600} />,
    );

    // Then
    byTestId(TEST_IDS.shortcutDragHandle).should("exist");
  });

  it("defaults to the upper-right edge of the screen", () => {
    // Given
    cy.mount(
      <ShortcutDrawer shortcuts={SAMPLE_SHORTCUTS} onSend={cy.stub()} viewportHeight={600} />,
    );

    // Then — the overlay starts snapped to the right edge near the top
    byTestId(TEST_IDS.shortcutDrawer).should("have.attr", "data-snap-edge", "right");
    byTestId(TEST_IDS.shortcutDrawer).then(($drawer) => {
      const rect = $drawer[0].getBoundingClientRect();
      expect(rect.right, "near the right edge").to.be.greaterThan(window.innerWidth - 60);
      expect(rect.top, "near the top").to.be.lessThan(60);
    });
  });

  it("snaps to the left edge when dragged into the left half of the screen", () => {
    // Given — the drawer starts at the right edge
    cy.mount(
      <div style={{ width: "100vw", height: "100vh" }}>
        <ShortcutDrawer shortcuts={SAMPLE_SHORTCUTS} onSend={cy.stub()} viewportHeight={600} />
      </div>,
    );
    byTestId(TEST_IDS.shortcutDrawer).should("have.attr", "data-snap-edge", "right");

    // When — drag the handle into the left half
    byTestId(TEST_IDS.shortcutDragHandle)
      .trigger("pointerdown", { clientX: 760, clientY: 100, pointerId: 1, force: true })
      .trigger("pointermove", { clientX: 120, clientY: 300, pointerId: 1, force: true })
      .trigger("pointerup", { clientX: 120, clientY: 300, pointerId: 1, force: true });

    // Then — it snaps to the left edge (and a drag does not expand it)
    byTestId(TEST_IDS.shortcutDrawer).should("have.attr", "data-snap-edge", "left");
    byTestId(TEST_IDS.shortcutDrawer).should("have.attr", "data-collapsed", "true");
  });

  it("snaps back to the right edge when dragged into the right half of the screen", () => {
    // Given — the drawer has been moved to the left edge
    cy.mount(
      <div style={{ width: "100vw", height: "100vh" }}>
        <ShortcutDrawer shortcuts={SAMPLE_SHORTCUTS} onSend={cy.stub()} viewportHeight={600} />
      </div>,
    );
    byTestId(TEST_IDS.shortcutDragHandle)
      .trigger("pointerdown", { clientX: 760, clientY: 100, pointerId: 1, force: true })
      .trigger("pointermove", { clientX: 120, clientY: 300, pointerId: 1, force: true })
      .trigger("pointerup", { clientX: 120, clientY: 300, pointerId: 1, force: true });
    byTestId(TEST_IDS.shortcutDrawer).should("have.attr", "data-snap-edge", "left");

    // When — drag the handle into the right half
    byTestId(TEST_IDS.shortcutDragHandle)
      .trigger("pointerdown", { clientX: 40, clientY: 300, pointerId: 1, force: true })
      .trigger("pointermove", { clientX: 700, clientY: 300, pointerId: 1, force: true })
      .trigger("pointerup", { clientX: 700, clientY: 300, pointerId: 1, force: true });

    // Then — it snaps to the right edge
    byTestId(TEST_IDS.shortcutDrawer).should("have.attr", "data-snap-edge", "right");
  });

  it("stacks buttons in a column when expanded", () => {
    // Given — an expanded drawer
    cy.mount(
      <ShortcutDrawer shortcuts={SAMPLE_SHORTCUTS} onSend={cy.stub()} viewportHeight={600} />,
    );
    tapShortcutControl();

    // Then — buttons share the same left coordinate and stack vertically (column layout)
    cy.get(`[data-testid='${shortcutButton("Shift+Tab")}']`).then(($first) => {
      cy.get(`[data-testid='${shortcutButton("Ctrl+C")}']`).then(($second) => {
        const firstRect = $first[0].getBoundingClientRect();
        const secondRect = $second[0].getBoundingClientRect();
        expect(Math.abs(firstRect.left - secondRect.left)).to.be.lessThan(4);
        expect(secondRect.top).to.be.greaterThan(firstRect.bottom - 1);
      });
    });
  });
});
