/**
 * Unit tests for the two translations input forwarding makes before anything reaches the wire.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-07-input-forwarding.md — AC-IF-2 (keysyms, special keys
 * included) and AC-IF-3 (a click lands where the operator aimed, at any window size).
 *
 * Held apart from the mounted-overlay acceptance tests on purpose: a component test can prove that
 * *a* coordinate reached the bridge, but the arithmetic that decides *which* coordinate is a pure
 * function and is cheaper and sharper to pin as one.
 */

import { describe, it, expect } from "bun:test";
import { framebufferPointFor, keysymFor } from "./screenSharingInput";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** A window showing a 1920×1080 desktop at half size — the ordinary case. */
const A_HALF_SIZE_WINDOW = { width: 960, height: 540 };
const A_FULL_HD_FRAMEBUFFER = { width: 1920, height: 1080 };

// ---------------------------------------------------------------------------
// AC-IF-3 — a click lands where the operator aimed
// ---------------------------------------------------------------------------

describe("the framebuffer point a pointer lands on", () => {
  it("doubles a pointer in a window showing the desktop at half size", () => {
    // Given a 1920×1080 desktop rendered into a 960×540 element
    // When the operator points at the middle of that element
    const landed = framebufferPointFor({ x: 480, y: 270 }, A_HALF_SIZE_WINDOW, A_FULL_HD_FRAMEBUFFER);

    // Then the desktop is pointed at in its own pixels, not the window's
    expect(landed).toEqual({ x: 960, y: 540 });
  });

  it("maps the same pointer to a different desktop pixel when the window is resized", () => {
    // Given the same aim, in a window a third of the desktop's size rather than half
    const landed = framebufferPointFor({ x: 480, y: 270 }, { width: 640, height: 360 }, A_FULL_HD_FRAMEBUFFER);

    // Then it lands further into the desktop — the mapping reads the window it is given, and is
    // not a constant factor baked in for one size
    expect(landed).toEqual({ x: 1440, y: 810 });
  });

  it("leaves the top-left corner at the origin", () => {
    // Given the operator points at the very corner of the picture
    const landed = framebufferPointFor({ x: 0, y: 0 }, A_HALF_SIZE_WINDOW, A_FULL_HD_FRAMEBUFFER);

    // Then that is the desktop's origin
    expect(landed).toEqual({ x: 0, y: 0 });
  });

  it("keeps the bottom-right corner inside the framebuffer it addresses", () => {
    // Given the operator points at the opposite corner
    const landed = framebufferPointFor(
      { x: 960, y: 540 },
      A_HALF_SIZE_WINDOW,
      A_FULL_HD_FRAMEBUFFER,
    );

    // Then it is the desktop's opposite corner, and not a pixel beyond it
    expect(landed).toEqual({ x: 1920, y: 1080 });
  });

  it("rounds a pointer that falls between two desktop pixels", () => {
    // Given a window whose width does not divide the framebuffer's: 1/300 of 1000 is 3.33…
    const landed = framebufferPointFor({ x: 1, y: 1 }, { width: 300, height: 300 }, { width: 1000, height: 1000 });

    // Then the nearest whole desktop pixel is addressed — the wire carries `uint32`, so some whole
    // pixel is going to be chosen and it should be the nearest one
    expect(landed).toEqual({ x: 3, y: 3 });
  });
});

// ---------------------------------------------------------------------------
// AC-IF-2 — the keys a desktop needs
// ---------------------------------------------------------------------------

describe("the keysym a key is forwarded as", () => {
  it("sends a printable letter as its own code point", () => {
    expect(keysymFor("a")).toBe(0x61);
  });

  it("distinguishes a shifted letter from an unshifted one", () => {
    // The two are different keysyms, not one keysym plus a modifier flag the wire does not carry.
    expect(keysymFor("A")).toBe(0x41);
    expect(keysymFor("a")).toBe(0x61);
  });

  it("sends Enter as XK_Return rather than as a character", () => {
    expect(keysymFor("Enter")).toBe(0xff0d);
  });

  it("sends Escape as XK_Escape, because a desktop is unusable without it", () => {
    expect(keysymFor("Escape")).toBe(0xff1b);
  });

  it("sends Tab as XK_Tab", () => {
    expect(keysymFor("Tab")).toBe(0xff09);
  });

  it("sends each arrow key as its own keysym", () => {
    expect(keysymFor("ArrowLeft")).toBe(0xff51);
    expect(keysymFor("ArrowUp")).toBe(0xff52);
    expect(keysymFor("ArrowRight")).toBe(0xff53);
    expect(keysymFor("ArrowDown")).toBe(0xff54);
  });

  it("sends the function keys as the consecutive keysyms X11 gives them", () => {
    expect(keysymFor("F1")).toBe(0xffbe);
    expect(keysymFor("F12")).toBe(0xffc9);
  });

  it("sends Backspace and Delete as the two different keys they are", () => {
    expect(keysymFor("Backspace")).toBe(0xff08);
    expect(keysymFor("Delete")).toBe(0xffff);
  });

  it("sends the modifiers a remote desktop has to see held down", () => {
    expect(keysymFor("Control")).toBe(0xffe3);
    expect(keysymFor("Alt")).toBe(0xffe9);
    expect(keysymFor("Shift")).toBe(0xffe1);
  });

  it("forwards nothing for a key it has no keysym for", () => {
    // A key with no mapping is dropped rather than sent as some stand-in the desktop would act on.
    expect(keysymFor("MediaPlayPause")).toBeNull();
    expect(keysymFor("BrowserBack")).toBeNull();
  });
});
