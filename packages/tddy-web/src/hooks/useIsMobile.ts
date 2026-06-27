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
  const [isMobile, setIsMobile] = useState(detectIsMobile);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const update = () => setIsMobile(detectIsMobile());
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, []);

  return isMobile;
}
