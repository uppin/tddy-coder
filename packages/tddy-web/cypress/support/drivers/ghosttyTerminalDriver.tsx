/**
 * Fluent component driver for GhosttyTerminal.
 *
 * Wraps mount → interact → assert into a chainable API so test bodies stay
 * free of raw selectors and React mounting boilerplate.
 *
 * Usage:
 *
 *   aGhosttyTerminal({ onData }).mount().click().expectExists();
 *   aGhosttyTerminal({ preventFocusOnTap: true }).mount().expectNoFocus();
 */

import React, { useRef } from "react";
import { mount } from "cypress/react";
import type { GhosttyTerminalProps, GhosttyTerminalHandle } from "../../../src/components/GhosttyTerminal";
import { GhosttyTerminal } from "../../../src/components/GhosttyTerminal";
import { byTestId, TEST_IDS } from "../testIds";

// ---------------------------------------------------------------------------
// Mobile keyboard wrapper used by preventFocusOnTap tests
// ---------------------------------------------------------------------------

function MobileKeyboardWrapper({
  onData,
  terminalProps,
}: {
  onData: (data: string) => void;
  terminalProps: Partial<GhosttyTerminalProps>;
}) {
  const ref = useRef<GhosttyTerminalHandle>(null);
  return (
    <>
      <GhosttyTerminal ref={ref} onData={onData} preventFocusOnTap {...terminalProps} />
      <button
        data-testid="keyboard-btn"
        type="button"
        onClick={() => ref.current?.focus()}
      >
        Keyboard
      </button>
    </>
  );
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

export interface GhosttyTerminalDriverOptions extends Partial<GhosttyTerminalProps> {
  /** When true, mounts via MobileKeyboardWrapper so ref.current.focus() can be tested. */
  withMobileKeyboardWrapper?: boolean;
  /** When true, mounts with a captured imperative handle so the live buffer (viewport + scrollback) can be inspected. */
  withHandleCapture?: boolean;
}

export function aGhosttyTerminal(options: GhosttyTerminalDriverOptions = {}) {
  const { withMobileKeyboardWrapper, withHandleCapture, ...terminalProps } = options;
  const handleRef = React.createRef<GhosttyTerminalHandle>();
  const onDataStub = terminalProps.onData ?? cy.stub().as("onData");
  const onResizeStub = terminalProps.onResize ?? undefined;

  const mergedProps: Partial<GhosttyTerminalProps> = {
    ...terminalProps,
    onData: typeof onDataStub === "function" ? onDataStub : undefined,
    onResize: onResizeStub,
  };

  const terminal = () => byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 });

  return {
    /** Mount the component (with optional wrapper for mobile-keyboard tests). */
    mount() {
      if (withMobileKeyboardWrapper) {
        mount(
          <MobileKeyboardWrapper
            onData={mergedProps.onData as (data: string) => void}
            terminalProps={mergedProps}
          />,
        );
      } else if (withHandleCapture) {
        mount(<GhosttyTerminal ref={handleRef} {...(mergedProps as GhosttyTerminalProps)} />);
      } else {
        mount(<GhosttyTerminal {...(mergedProps as GhosttyTerminalProps)} />);
      }
      return this;
    },

    /** Wait for the terminal element to exist. */
    expectExists() {
      terminal().should("exist");
      return this;
    },

    /** Assert the terminal contains a canvas element. */
    expectCanvasExists() {
      terminal().within(() => cy.get("canvas").should("exist"));
      return this;
    },

    /** Click the terminal area. */
    click(position?: Cypress.PositionType) {
      if (position) terminal().click(position);
      else terminal().click();
      return this;
    },

    /** Type into the terminal. */
    type(text: string) {
      terminal().type(text);
      return this;
    },

    /**
     * Simulate a physical key press (with modifiers) arriving at the document
     * while the terminal window is the active context — e.g. a desktop user
     * pressing Shift+Tab or Alt+M with the terminal focused. Dispatched on
     * `document` (not the inner textarea) so it exercises an app-level shortcut
     * handler rather than ghostty-web's own textarea key mapping.
     */
    pressPhysicalKey(init: {
      key: string;
      code?: string;
      shiftKey?: boolean;
      altKey?: boolean;
      ctrlKey?: boolean;
      metaKey?: boolean;
    }) {
      cy.document().then((doc) => {
        doc.dispatchEvent(
          new KeyboardEvent("keydown", {
            bubbles: true,
            cancelable: true,
            shiftKey: false,
            altKey: false,
            ctrlKey: false,
            metaKey: false,
            ...init,
          }),
        );
      });
      return this;
    },

    /** Assert the `@onData` stub was called (at least once). */
    expectOnDataCalled() {
      cy.get("@onData").should("have.been.called");
      return this;
    },

    /** Assert the `@onData` stub was called with a specific value. */
    expectOnDataCalledWith(value: string) {
      cy.get("@onData").should("have.been.calledWith", value);
      return this;
    },

    /** Assert the `@onResize` stub was called. */
    expectOnResizeCalled(timeout = 5000) {
      cy.get("@onResize", { timeout }).should("have.been.called");
      return this;
    },

    /**
     * Assert that no element inside the terminal has document focus —
     * used by preventFocusOnTap tests.
     * Uses .should() so Cypress retries until the assertion passes or times out.
     */
    expectNoFocus(message = "terminal should not have focus") {
      terminal().should(($term) => {
        const active = $term[0].ownerDocument.activeElement;
        expect($term[0].contains(active), message).to.be.false;
      });
      return this;
    },

    /** Click the "Keyboard" button in the MobileKeyboardWrapper. */
    focusViaKeyboardButton() {
      byTestId("keyboard-btn").click();
      return this;
    },

    /** Synthesise a touch tap on the terminal. */
    simulateTouchTap() {
      terminal().then(($el) => {
        const el = $el[0];
        const rect = el.getBoundingClientRect();
        const touch = new Touch({
          identifier: 1,
          target: el,
          clientX: rect.left + rect.width / 2,
          clientY: rect.top + rect.height / 2,
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
          }),
        );
        el.dispatchEvent(
          new TouchEvent("touchend", {
            touches: [],
            targetTouches: [],
            changedTouches: [touch],
            cancelable: true,
          }),
        );
      });
      return this;
    },

    /**
     * Simulate a single-finger drag downward over the terminal — the natural,
     * content-following gesture a mobile user makes to pull earlier output back
     * into view. Dispatches a touchstart, a series of downward touchmoves, and a
     * touchend on the terminal container. Requires `withHandleCapture: true` so
     * scroll offset can be read.
     */
    dragDownOneFinger() {
      terminal().then(($el) => {
        const el = $el[0];
        const rect = el.getBoundingClientRect();
        const cx = rect.left + rect.width / 2;
        const startY = rect.top + rect.height * 0.2;
        const touchAt = (y: number) =>
          new Touch({
            identifier: 1,
            target: el,
            clientX: cx,
            clientY: y,
            radiusX: 0,
            radiusY: 0,
            rotationAngle: 0,
            force: 1,
          });
        const start = touchAt(startY);
        el.dispatchEvent(
          new TouchEvent("touchstart", {
            touches: [start],
            targetTouches: [start],
            changedTouches: [start],
            cancelable: true,
            bubbles: true,
          }),
        );
        for (let step = 1; step <= 6; step++) {
          const moved = touchAt(startY + step * 20);
          el.dispatchEvent(
            new TouchEvent("touchmove", {
              touches: [moved],
              targetTouches: [moved],
              changedTouches: [moved],
              cancelable: true,
              bubbles: true,
            }),
          );
        }
        const end = touchAt(startY + 120);
        el.dispatchEvent(
          new TouchEvent("touchend", {
            touches: [],
            targetTouches: [],
            changedTouches: [end],
            cancelable: true,
            bubbles: true,
          }),
        );
      });
      return this;
    },

    /**
     * Assert the terminal viewport has scrolled back from the bottom — i.e. an
     * earlier region of output is now visible. `.should(cb)` retries so the
     * assertion absorbs the async settle after the gesture.
     */
    expectRevealsEarlierOutput() {
      cy.wrap(handleRef).should((ref) => {
        const handle = (ref as unknown as React.RefObject<GhosttyTerminalHandle>).current;
        const offset = handle?.getViewportScrollOffset?.() ?? 0;
        expect(offset, "terminal viewport should reveal earlier output after the drag").to.be.greaterThan(0);
      });
      return this;
    },

    /**
     * Raw access to the terminal Cypress chain for assertions not covered by
     * the driver methods.
     */
    el() {
      return terminal();
    },
  };
}
