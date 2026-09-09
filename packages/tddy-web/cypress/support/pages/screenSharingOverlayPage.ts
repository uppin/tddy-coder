/**
 * The remote-desktop overlay, as an operator touches it.
 *
 * Both scopes — a session's screen-sharing tab and a host row's desktop — mount the same
 * `ScreenSharingOverlay`, so both drive it through this one page object.
 *
 * Gestures are expressed as an offset **inside the rendered picture** ("press at 480,270 in this
 * window") and translated here into the client coordinates a browser event carries. That is
 * deliberate: the offset is what the operator aimed at, and turning it into `clientX`/`clientY` is
 * DOM mechanics a spec should not be spelling out — nor should a spec accidentally hand the
 * component the very number it is supposed to compute.
 */

import { byTestId } from "../testIds";

export interface Size {
  readonly width: number;
  readonly height: number;
}

export interface Point {
  readonly x: number;
  readonly y: number;
}

const video = () => byTestId("screen-sharing-overlay-video");

/** Dispatch a mouse event at an offset inside the rendered picture. */
function mouseAt(eventName: string, at: Point, options: Record<string, unknown>) {
  video().then(($el) => {
    const box = $el[0].getBoundingClientRect();
    cy.wrap($el, { log: false }).trigger(eventName, {
      clientX: box.left + at.x,
      clientY: box.top + at.y,
      force: true,
      ...options,
    });
  });
}

/** Dispatch a keyboard event on the desktop picture; it bubbles to whatever is listening. */
function key(eventName: "keydown" | "keyup", keyName: string, modifiers: Record<string, boolean>) {
  video().trigger(eventName, {
    key: keyName,
    bubbles: true,
    force: true,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...modifiers,
  });
}

export const desktopPage = {
  /** The overlay itself — present exactly while a desktop is open. */
  root: () => byTestId("screen-sharing-overlay"),
  /** The picture. Present whether or not input works, which is the whole of AC-IF-8. */
  picture: video,
  close: () => byTestId("screen-sharing-overlay-close"),
  /** The overlay's statement that this desktop is watchable but not touchable. */
  inputUnavailableNotice: () => byTestId("screen-sharing-overlay-input-unavailable"),

  /**
   * Render the picture at a stated size, and confirm it took.
   *
   * The confirmation is not ceremony: every coordinate assertion in these specs is computed against
   * this size, so a fixture that silently failed to apply it would leave the scaling tests
   * asserting arithmetic about a window that never existed.
   */
  renderedAt(size: Size) {
    video().invoke("css", { width: `${size.width}px`, height: `${size.height}px` });
    video().should(($el) => {
      const box = $el[0].getBoundingClientRect();
      expect(Math.round(box.width), "the width the desktop is rendered at").to.equal(size.width);
      expect(Math.round(box.height), "the height the desktop is rendered at").to.equal(size.height);
    });
  },

  movePointerTo(at: Point) {
    mouseAt("mousemove", at, { button: 0, buttons: 0 });
  },

  pressLeftButtonAt(at: Point) {
    mouseAt("mousedown", at, { button: 0, buttons: 1 });
  },

  releaseLeftButtonAt(at: Point) {
    mouseAt("mouseup", at, { button: 0, buttons: 0 });
  },

  pressRightButtonAt(at: Point) {
    mouseAt("mousedown", at, { button: 2, buttons: 2 });
  },

  /**
   * Let a held button go somewhere that is not the picture.
   *
   * The picture keeps its aspect ratio inside a full-screen container, so there is always letterbox
   * around it — press inside, drag out, release there is an ordinary thing to do with a mouse, and
   * the release never reaches the picture's own listeners. Dispatched on `body` at a point measured
   * to be outside the picture, so this gesture cannot quietly become an inside-the-picture one if
   * the layout changes.
   */
  releaseLeftButtonOutsideThePicture() {
    video().then(($el) => {
      const box = $el[0].getBoundingClientRect();
      const outside = { clientX: box.left + box.width / 2, clientY: box.top - 10 };
      expect(outside.clientY, "a point in the letterbox above the picture").to.be.lessThan(box.top);
      expect(outside.clientY, "a point still inside the window").to.be.at.least(0);
      cy.get("body").trigger("mouseup", {
        ...outside,
        button: 0,
        buttons: 0,
        force: true,
      });
    });
  },

  pressKey(keyName: string, modifiers: Record<string, boolean> = {}) {
    key("keydown", keyName, modifiers);
  },

  /** Press and release one key — what pressing a key on a keyboard actually is. */
  tapKey(keyName: string, modifiers: Record<string, boolean> = {}) {
    key("keydown", keyName, modifiers);
    key("keyup", keyName, modifiers);
  },

  /**
   * Assert the overlay names the one chord it keeps for itself.
   *
   * Matched as a pattern: what has to be true is that an operator can find the way out without
   * having memorised it, not which sentence says so.
   */
  expectNamesTheClosingChord() {
    desktopPage
      .root()
      .invoke("text")
      .should("match", /ctrl[^a-z]*\+?[^a-z]*alt[^a-z]*\+?[^a-z]*esc/i);
  },
};
