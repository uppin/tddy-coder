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

/** A mouse event observed reaching the terminal, reduced to the fields tap-to-click cares about. */
interface RecordedMouseEvent {
  type: string;
  clientX: number;
  clientY: number;
}

/** Viewport coordinates a synthesised touch was delivered at. */
interface TapPoint {
  clientX: number;
  clientY: number;
}

export function aGhosttyTerminal(options: GhosttyTerminalDriverOptions = {}) {
  const { withMobileKeyboardWrapper, withHandleCapture, ...terminalProps } = options;
  const handleRef = React.createRef<GhosttyTerminalHandle>();
  /** Mouse events seen since `recordMouseEvents()` — mutated in place so `.should()` retries observe growth. */
  const recordedMouseEvents: RecordedMouseEvent[] = [];
  /** Where the last synthesised tap landed, for coordinate-fidelity assertions. */
  let lastTapPoint: TapPoint | null = null;
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

    /** Type after focus was set imperatively (e.g. via the mobile Keyboard button on a
     *  `preventFocusOnTap` terminal). `cy.type` normally click-to-focuses its target first, but
     *  `preventFocusOnTap` blocks click-focus by design — so the click-to-focus leaves
     *  `document.activeElement` unchanged and `cy.type` rejects the element as "disabled". `force`
     *  skips that click-to-focus actionability check (focus is already established imperatively)
     *  and dispatches the keystroke on the terminal host, which is where ghostty-web's key listener
     *  lives. */
    typeAfterImperativeFocus(text: string) {
      terminal().type(text, { force: true });
      return this;
    },

    /** Synthesise a touch tap at the centre of the terminal. */
    simulateTouchTap() {
      return this.simulateTouchTapAt(0.5, 0.5);
    },

    /**
     * Synthesise a touch tap at a fraction of the terminal's box — `(0.5, 0.5)` is the centre,
     * `(0.25, 0.75)` is left-of-centre and low. Records the resulting viewport coordinates so
     * `expectMouseDownAtTapPoint()` can check the click landed where the finger did.
     */
    simulateTouchTapAt(xRatio: number, yRatio: number) {
      terminal().then(($el) => {
        const el = $el[0];
        const rect = el.getBoundingClientRect();
        const point: TapPoint = {
          clientX: rect.left + rect.width * xRatio,
          clientY: rect.top + rect.height * yRatio,
        };
        lastTapPoint = point;
        const touch = new Touch({
          identifier: 1,
          target: el,
          clientX: point.clientX,
          clientY: point.clientY,
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
     * Synthesise a two-finger touch at the centre of the terminal: one finger lands, a second
     * joins, then both lift one after the other — the shape of a pinch or a two-finger tap.
     */
    tapWithTwoFingers() {
      terminal().then(($el) => {
        const el = $el[0];
        const rect = el.getBoundingClientRect();
        const midY = rect.top + rect.height / 2;
        const fingerAt = (identifier: number, clientX: number) =>
          new Touch({
            identifier,
            target: el,
            clientX,
            clientY: midY,
            radiusX: 0,
            radiusY: 0,
            rotationAngle: 0,
            force: 1,
          });
        const first = fingerAt(1, rect.left + rect.width * 0.4);
        const second = fingerAt(2, rect.left + rect.width * 0.6);
        const touchEvent = (type: string, touches: Touch[], changedTouches: Touch[]) =>
          new TouchEvent(type, {
            touches,
            targetTouches: touches,
            changedTouches,
            cancelable: true,
            bubbles: true,
          });
        el.dispatchEvent(touchEvent("touchstart", [first], [first]));
        el.dispatchEvent(touchEvent("touchstart", [first, second], [second]));
        el.dispatchEvent(touchEvent("touchend", [first], [second]));
        el.dispatchEvent(touchEvent("touchend", [], [first]));
      });
      return this;
    },

    /**
     * Start recording mouse events that reach the terminal. Listens in the capture phase on the
     * container, so events dispatched at the inner canvas are observed whether or not they bubble.
     * Call before the gesture under test.
     */
    recordMouseEvents() {
      terminal().then(($el) => {
        const el = $el[0];
        const record = (event: Event) => {
          const mouse = event as MouseEvent;
          recordedMouseEvents.push({
            type: mouse.type,
            clientX: mouse.clientX,
            clientY: mouse.clientY,
          });
        };
        el.addEventListener("mousedown", record, { capture: true });
        el.addEventListener("mouseup", record, { capture: true });
        el.addEventListener("click", record, { capture: true });
      });
      return this;
    },

    /** Assert the recorded press/release events are exactly one mousedown followed by one mouseup. */
    expectMouseDownThenMouseUp() {
      cy.wrap(null).should(() => {
        const pressReleases = recordedMouseEvents
          .map((event) => event.type)
          .filter((type) => type === "mousedown" || type === "mouseup");
        expect(pressReleases, "a tap should reach the terminal as a mousedown/mouseup pair").to.deep.equal([
          "mousedown",
          "mouseup",
        ]);
      });
      return this;
    },

    /** Assert exactly one `click` event reached the terminal. */
    expectClickDispatchedOnce() {
      cy.wrap(null).should(() => {
        const clicks = recordedMouseEvents.filter((event) => event.type === "click");
        expect(clicks.length, "a tap should reach the terminal as a single click event").to.equal(1);
      });
      return this;
    },

    /** Assert no `click` event reached the terminal — a gesture that is not a tap must not click. */
    expectNoClickDispatched() {
      cy.wrap(null).should(() => {
        const clicks = recordedMouseEvents.filter((event) => event.type === "click");
        expect(clicks.length, "a gesture that is not a tap should not reach the terminal as a click").to.equal(0);
      });
      return this;
    },

    /** Assert the recorded mousedown carries the coordinates the finger touched. */
    expectMouseDownAtTapPoint() {
      cy.wrap(null).should(() => {
        expect(lastTapPoint, "simulate a tap before asserting where its click landed").to.not.equal(null);
        const [mouseDown] = recordedMouseEvents.filter((event) => event.type === "mousedown");
        expect(mouseDown, "a tap should reach the terminal as a mousedown").to.not.equal(undefined);
        expect({ clientX: mouseDown.clientX, clientY: mouseDown.clientY }).to.deep.equal(lastTapPoint);
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
     * Wait until the terminal is wired to report mouse gestures to the TUI: the DECSET
     * mouse-tracking sequences have been processed, *and* the effect that attaches the
     * mouse/touch listeners has committed. The terminal enables tracking while still inside its
     * async setup — one render before that effect runs — so the two frames flush the commit
     * (useEffect runs after paint) instead of guessing at a delay.
     * Requires `withHandleCapture: true`.
     */
    expectReportingMouseToTui() {
      cy.wrap(handleRef).should((ref) => {
        const handle = (ref as unknown as React.RefObject<GhosttyTerminalHandle>).current;
        expect(
          handle?.hasMouseTracking?.() ?? false,
          "terminal should have mouse tracking enabled by the TUI",
        ).to.equal(true);
      });
      cy.wrap(null).then(
        () =>
          new Promise<void>((resolve) => {
            requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
          }),
      );
      return this;
    },

    /**
     * Assert `@onData` carried exactly one SGR mouse press (`…M`) and exactly one release (`…m`) —
     * a gesture must never be reported to the TUI twice.
     */
    expectSgrPressAndReleaseReportedOnce() {
      cy.get("@onData").should((subject) => {
        const stub = subject as unknown as { getCalls: () => { args: unknown[] }[] };
        const sgr = stub
          .getCalls()
          .map((call) => call.args[0])
          .filter((arg): arg is string => typeof arg === "string")
          .filter((data) => /^\x1b\[<\d+;\d+;\d+[Mm]$/.test(data));
        const reports = {
          presses: sgr.filter((data) => data.endsWith("M")).length,
          releases: sgr.filter((data) => data.endsWith("m")).length,
        };
        expect(reports, "a tap should be reported as one SGR press and one release").to.deep.equal({
          presses: 1,
          releases: 1,
        });
      });
      return this;
    },

    /**
     * Wait until the terminal is in the alternate screen (DEC 1049) — the full-screen TUI mode —
     * and the effects that read that state have committed. Same two-frame flush as
     * `expectReportingMouseToTui`, for the same reason. Requires `withHandleCapture: true`.
     */
    expectInAlternateScreen() {
      cy.wrap(handleRef).should((ref) => {
        const handle = (ref as unknown as React.RefObject<GhosttyTerminalHandle>).current;
        expect(
          handle?.isAlternateScreen?.() ?? false,
          "terminal should be in the alternate screen (DEC 1049)",
        ).to.equal(true);
      });
      cy.wrap(null).then(
        () =>
          new Promise<void>((resolve) => {
            requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
          }),
      );
      return this;
    },

    /** Assert the bytes the terminal sent to the application (`@onData`) include `substr`. */
    expectSentToApp(substr: string) {
      cy.get("@onData", { timeout: 4000 }).should((subject) => {
        const stub = subject as unknown as { getCalls: () => { args: unknown[] }[] };
        const sent = stub
          .getCalls()
          .map((call) => call.args[0])
          .filter((arg): arg is string => typeof arg === "string")
          .join("");
        expect(sent, "bytes sent to the application").to.include(substr);
      });
      return this;
    },

    /** Assert the bytes the terminal sent to the application (`@onData`) do NOT include `substr`. */
    expectDidNotSendToApp(substr: string) {
      cy.get("@onData", { timeout: 4000 }).should((subject) => {
        const stub = subject as unknown as { getCalls: () => { args: unknown[] }[] };
        const sent = stub
          .getCalls()
          .map((call) => call.args[0])
          .filter((arg): arg is string => typeof arg === "string")
          .join("");
        expect(sent, "bytes sent to the application").to.not.include(substr);
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
