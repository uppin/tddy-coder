import { useEffect, useState } from "react";

/** Viewport widths below this (the Tailwind `md` breakpoint) are treated as mobile. */
export const MOBILE_MAX_WIDTH_PX = 768;

/**
 * One-shot check for whether the current device should use the mobile layout:
 * touch-capable devices, or viewports narrower than the `md` breakpoint.
 *
 * SSR-safe — returns `false` when `window` is unavailable. Use this directly
 * (rather than {@link useIsMobile}) when you only need the value once, e.g. as
 * the lazy initial value of a `useState`.
 */
export function detectIsMobile(): boolean {
  if (typeof window === "undefined") return false;
  return "ontouchstart" in window || window.innerWidth < MOBILE_MAX_WIDTH_PX;
}

/**
 * Reactive variant of {@link detectIsMobile} — re-evaluates on viewport resize
 * (e.g. window resizing or orientation changes).
 */
export function useIsMobile(): boolean {
  return useViewportFlag(detectIsMobile);
}

/** Whether a viewport this wide is below the `md` breakpoint. */
export function isNarrowWidth(innerWidth: number): boolean {
  return innerWidth < MOBILE_MAX_WIDTH_PX;
}

/**
 * Width-only variant of {@link useIsMobile}: true below the `md` breakpoint whatever the input
 * device. For a layout that depends on room alone — a docked column — where a touch-capable
 * desktop at full width must keep its column rather than take the mobile overlay.
 */
export function useIsNarrowViewport(): boolean {
  return useViewportFlag(detectIsNarrowViewport);
}

function detectIsNarrowViewport(): boolean {
  return typeof window !== "undefined" && isNarrowWidth(window.innerWidth);
}

/** `detect()` now, re-evaluated on every viewport resize (window resizing, orientation changes). */
function useViewportFlag(detect: () => boolean): boolean {
  const [flag, setFlag] = useState(detect);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const update = () => setFlag(detect());
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, [detect]);

  return flag;
}
