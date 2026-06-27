import { afterEach, describe, expect, it } from "bun:test";
import { detectIsMobile, MOBILE_MAX_WIDTH_PX } from "./useIsMobile";

const originalWindow = (globalThis as Record<string, unknown>).window;

function setWindow(win: unknown): void {
  (globalThis as Record<string, unknown>).window = win;
}

afterEach(() => {
  setWindow(originalWindow);
});

describe("detectIsMobile", () => {
  it("reports desktop when window is unavailable (SSR)", () => {
    // Given
    setWindow(undefined);

    // When / Then
    expect(detectIsMobile()).toBe(false);
  });

  it("treats a viewport narrower than the md breakpoint as mobile", () => {
    // Given
    setWindow({ innerWidth: MOBILE_MAX_WIDTH_PX - 1 });

    // When / Then
    expect(detectIsMobile()).toBe(true);
  });

  it("treats a viewport at the md breakpoint without touch as desktop", () => {
    // Given
    setWindow({ innerWidth: MOBILE_MAX_WIDTH_PX });

    // When / Then
    expect(detectIsMobile()).toBe(false);
  });

  it("treats a touch-capable device as mobile regardless of width", () => {
    // Given — a wide viewport that nonetheless exposes touch support
    setWindow({ innerWidth: 1920, ontouchstart: null });

    // When / Then
    expect(detectIsMobile()).toBe(true);
  });
});
