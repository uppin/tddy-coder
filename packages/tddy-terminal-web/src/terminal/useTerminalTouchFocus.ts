import { useEffect } from "react";
import { TOUCH_SCROLL_START_THRESHOLD_PX } from "./useTerminalTouchScroll";
import type { TerminalLoggers } from "./terminalLoggers";
import type { ContainerRef, TerminalRef } from "./terminalRefs";

/**
 * Keeps the hidden textarea in step with `sessionActive` and `preventFocusOnTap`: disabled when the
 * session is gone, read-only so the mobile keyboard does not open on tap.
 */
export function useTerminalTextareaAttrs({
  termRef,
  ready,
  sessionActive,
  preventFocusOnTap,
}: {
  termRef: TerminalRef;
  ready: boolean;
  sessionActive: boolean;
  preventFocusOnTap: boolean;
}): void {
  useEffect(() => {
    if (!ready || !termRef.current?.inputElement) return;
    const ta = termRef.current.inputElement;
    if (sessionActive) {
      ta.removeAttribute("aria-disabled");
    } else {
      ta.setAttribute("aria-disabled", "true");
      ta.blur();
    }
  }, [sessionActive, ready]);

  // Set textarea readonly when preventFocusOnTap — prevents mobile keyboard from opening on tap
  useEffect(() => {
    if (!ready || !termRef.current) return;
    const textarea = termRef.current.inputElement;
    if (!textarea) return;
    if (preventFocusOnTap) {
      textarea.setAttribute("readonly", "");
    } else {
      textarea.removeAttribute("readonly");
    }
  }, [ready, preventFocusOnTap]);
}

/**
 * Prevent focus on pointer/touch/click when `preventFocusOnTap` (e.g. mobile, keyboard closed).
 */
export function usePreventFocusOnTap({
  containerRef,
  preventFocusOnTap,
}: {
  containerRef: ContainerRef;
  preventFocusOnTap: boolean;
}): void {
  // Prevent focus on pointer/touch/click when preventFocusOnTap (e.g. mobile, keyboard closed).
  useEffect(() => {
    const container = containerRef.current;
    if (!container || !preventFocusOnTap) return;
    const preventFocus = (e: Event) => {
      e.preventDefault();
      // Blur after all handlers run — xterm.js focuses its textarea in its mousedown handler
      queueMicrotask(() => {
        const active = document.activeElement;
        if (active && container.contains(active)) {
          (active as HTMLElement).blur();
        }
      });
    };
    container.addEventListener("pointerdown", preventFocus, { capture: true });
    container.addEventListener("mousedown", preventFocus, { capture: true });
    container.addEventListener("touchstart", preventFocus, { capture: true, passive: false });
    container.addEventListener("click", preventFocus, { capture: true });
    return () => {
      container.removeEventListener("pointerdown", preventFocus, { capture: true });
      container.removeEventListener("mousedown", preventFocus, { capture: true });
      container.removeEventListener("touchstart", preventFocus, { capture: true });
      container.removeEventListener("click", preventFocus, { capture: true });
    };
  }, [preventFocusOnTap]);
}

/** Re-synthesises the compatibility mouse events a cancelled `touchstart` withheld from a tap. */
export function useTapAsClick({
  containerRef,
  preventFocusOnTap,
  loggers,
}: {
  containerRef: ContainerRef;
  preventFocusOnTap: boolean;
  loggers: TerminalLoggers;
}): void {
  const { logMouse } = loggers;
  // A tap must still reach the terminal as a mouse click. The effect above cancels `touchstart`
  // to keep focus (and the mobile keyboard) away, and a cancelled `touchstart` also suppresses
  // the browser's compatibility mouse events — so without this a tap produces no `mousedown`,
  // `mouseup` or `click` at all, and neither xterm.js's own click handling nor a TUI that never
  // enables mouse reporting (the Claude CLI emits no DECSET 1000/1002/1003/1006) sees the
  // gesture. Re-synthesise the compatibility events the browser withheld, at the touch point.
  // Those events are the tap's only route to a TUI that does track the mouse as well: xterm.js
  // reports them as SGR press and release itself.
  useEffect(() => {
    const container = containerRef.current;
    if (!container || !preventFocusOnTap) return;

    /** The in-flight gesture while it still looks like a tap; cleared once it drags or gains a finger. */
    let tap: { clientX: number; clientY: number } | null = null;

    const onTouchStart = (e: TouchEvent) => {
      // A second finger (pinch) means the gesture is no longer a tap.
      if (e.touches.length !== 1 || e.changedTouches.length !== 1) {
        tap = null;
        return;
      }
      const touch = e.changedTouches[0];
      tap = { clientX: touch.clientX, clientY: touch.clientY };
    };

    const onTouchMove = (e: TouchEvent) => {
      if (!tap || e.changedTouches.length === 0) return;
      const touch = e.changedTouches[0];
      const travelled = Math.hypot(touch.clientX - tap.clientX, touch.clientY - tap.clientY);
      // Past the threshold the drag-to-scroll effect engages at, this is a scroll, not a tap.
      if (travelled >= TOUCH_SCROLL_START_THRESHOLD_PX) tap = null;
    };

    const onTouchEnd = (e: TouchEvent) => {
      const candidate = tap;
      tap = null;
      if (!candidate) return;
      // Fingers still down, or several lifting at once — the tail of a multi-touch gesture.
      if (e.touches.length > 0 || e.changedTouches.length !== 1) return;
      const touch = e.changedTouches[0];
      const init: MouseEventInit = {
        bubbles: true,
        cancelable: true,
        view: window,
        button: 0,
        detail: 1,
        clientX: touch.clientX,
        clientY: touch.clientY,
      };
      logMouse("tap synthesised as click at (%o,%o)", touch.clientX, touch.clientY);
      // Dispatch at the element the finger landed on (what a real compatibility event targets),
      // so xterm.js's screen handlers see the click, not just the container.
      touch.target.dispatchEvent(new MouseEvent("mousedown", { ...init, buttons: 1 }));
      touch.target.dispatchEvent(new MouseEvent("mouseup", init));
      touch.target.dispatchEvent(new MouseEvent("click", init));
    };

    const cancelTap = () => {
      tap = null;
    };

    container.addEventListener("touchstart", onTouchStart, { passive: true });
    container.addEventListener("touchmove", onTouchMove, { passive: true });
    container.addEventListener("touchend", onTouchEnd, { passive: true });
    container.addEventListener("touchcancel", cancelTap, { passive: true });

    return () => {
      container.removeEventListener("touchstart", onTouchStart);
      container.removeEventListener("touchmove", onTouchMove);
      container.removeEventListener("touchend", onTouchEnd);
      container.removeEventListener("touchcancel", cancelTap);
    };
  }, [preventFocusOnTap]);
}
