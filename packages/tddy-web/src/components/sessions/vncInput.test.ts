import { describe, it, expect } from "bun:test";
import {
  keyboardEventToKeysym,
  mouseButtonToRfbMask,
  scaleCoordinates,
  wheelDeltaToRfbMask,
} from "./vncInput";

// Tests for VNC input mapping helpers.

describe("scaleCoordinates", () => {
  it("scales x and y proportionally from video space to framebuffer space", () => {
    // Given — video is 400×200, framebuffer is 800×600
    // Pointer at (100, 50) in video space
    //   scaled x: 100/400 * 800 = 200
    //   scaled y: 50/200  * 600 = 150

    // When
    const result = scaleCoordinates(100, 50, 400, 200, 800, 600);

    // Then
    expect(result.x).toBe(200);
    expect(result.y).toBe(150);
  });

  it("returns (0, 0) when the pointer is at the top-left corner", () => {
    const result = scaleCoordinates(0, 0, 640, 480, 1280, 960);
    expect(result.x).toBe(0);
    expect(result.y).toBe(0);
  });

  it("returns (framebufferWidth, framebufferHeight) when pointer is at the bottom-right", () => {
    const result = scaleCoordinates(640, 480, 640, 480, 1280, 960);
    expect(result.x).toBe(1280);
    expect(result.y).toBe(960);
  });

  it("handles non-integer scaled coordinates by rounding to nearest integer", () => {
    // video 300 wide, fb 1000 wide: pointer at x=1 → 1/300 * 1000 ≈ 3.33 → 3
    const result = scaleCoordinates(1, 0, 300, 200, 1000, 600);
    expect(result.x).toBe(3);
  });
});

describe("keyboardEventToKeysym", () => {
  it("maps the 'a' key to keysym 0x61", () => {
    const event = new KeyboardEvent("keydown", { key: "a", code: "KeyA" });
    expect(keyboardEventToKeysym(event)).toBe(0x61);
  });

  it("maps the 'A' key (shift+a) to keysym 0x41", () => {
    const event = new KeyboardEvent("keydown", { key: "A", code: "KeyA", shiftKey: true });
    expect(keyboardEventToKeysym(event)).toBe(0x41);
  });

  it("maps the Enter key to keysym 0xff0d (XK_Return)", () => {
    const event = new KeyboardEvent("keydown", { key: "Enter", code: "Enter" });
    expect(keyboardEventToKeysym(event)).toBe(0xff0d);
  });

  it("maps the Escape key to keysym 0xff1b (XK_Escape)", () => {
    const event = new KeyboardEvent("keydown", { key: "Escape", code: "Escape" });
    expect(keyboardEventToKeysym(event)).toBe(0xff1b);
  });

  it("returns null for an unrecognised special key", () => {
    const event = new KeyboardEvent("keydown", { key: "F20", code: "F20" });
    expect(keyboardEventToKeysym(event)).toBeNull();
  });
});

describe("mouseButtonToRfbMask", () => {
  it("maps left button (0) to RFB mask bit 0 = 1", () => {
    expect(mouseButtonToRfbMask(0)).toBe(1);
  });

  it("maps middle button (1) to RFB mask bit 1 = 2", () => {
    expect(mouseButtonToRfbMask(1)).toBe(2);
  });

  it("maps right button (2) to RFB mask bit 2 = 4", () => {
    expect(mouseButtonToRfbMask(2)).toBe(4);
  });
});

describe("wheelDeltaToRfbMask", () => {
  it("returns button-4 mask (16) for scroll-up (negative deltaY)", () => {
    expect(wheelDeltaToRfbMask(-100)).toBe(16);
  });

  it("returns button-5 mask (32) for scroll-down (positive deltaY)", () => {
    expect(wheelDeltaToRfbMask(100)).toBe(32);
  });

  it("returns 0 for no scroll (deltaY = 0)", () => {
    expect(wheelDeltaToRfbMask(0)).toBe(0);
  });
});
