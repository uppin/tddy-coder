import { useCallback, useEffect } from "react";
import { clientPointToTerminalCell } from "../lib/terminalMouseCellCoords";
import { measureTerminalCanvasCssHeight } from "./terminalGeometry";
import type { TerminalLoggers } from "./terminalLoggers";
import type { ContainerRef, OnDataRef, TerminalRef } from "./terminalRefs";

/** Movement (px) a single finger must travel before a touch counts as a drag rather than a tap.
 *  The scroll effect measures it vertically; the tap detector measures total travel. */
export const TOUCH_SCROLL_START_THRESHOLD_PX = 8;

/** Scroll notches one `touchmove` may hand to the TUI — a flung finger must not flood it. */
const TOUCH_SCROLL_MAX_NOTCHES_PER_MOVE = 8;

/** Report one wheel notch at a viewport point to the TUI as an SGR mouse event (button 64 up /
 *  65 down). False when the point is off the grid or nothing can receive it. Shared by the
 *  `sendWheelSgr` handle method (desktop wheel) and the touch drag route below, so both report
 *  a scroll the same way. */
export function useSendWheelSgrAt({
  termRef,
  onDataRef,
}: {
  termRef: TerminalRef;
  onDataRef: OnDataRef;
}): (clientX: number, clientY: number, up: boolean) => boolean {
  return useCallback((clientX: number, clientY: number, up: boolean): boolean => {
    const t = termRef.current;
    const send = onDataRef.current;
    if (!t || !send) return false;
    const canvas = t.element?.querySelector("canvas");
    if (!canvas) return false;
    const r = canvas.getBoundingClientRect();
    const grid = { left: r.left, top: r.top, width: r.width, height: r.height };
    const coords = clientPointToTerminalCell(clientX, clientY, grid, t.cols, t.rows);
    if (!coords) return false;
    send(`\x1b[<${up ? 64 : 65};${coords.col};${coords.row}M`);
    return true;
  }, []);
}

/**
 * Single-finger vertical drag → scroll, routed by what is on screen (see the long note below).
 */
export function useTouchScrollRoutes({
  containerRef,
  termRef,
  onDataRef,
  sendWheelSgrAt,
  sessionActive,
  fixedViewportGrid,
  loggers,
}: {
  containerRef: ContainerRef;
  termRef: TerminalRef;
  onDataRef: OnDataRef;
  sendWheelSgrAt: (clientX: number, clientY: number, up: boolean) => boolean;
  sessionActive: boolean;
  fixedViewportGrid: { cols: number; rows: number } | undefined;
  loggers: TerminalLoggers;
}): void {
  const { logMouse, logScroll } = loggers;
  // Single-finger vertical drag → scroll. Mobile has no mouse wheel, so a swipe is the
  // only scroll gesture there. Natural, content-following direction: dragging the finger
  // down pulls older output into view.
  //
  // Who owns the gesture depends on what is on screen, the same three ways the wheel is gated on
  // desktop (`GhosttyTerminalSession`'s capture-phase listener plus ghostty-web's own wheel
  // handling). Each line of finger travel is one notch of that gesture:
  //
  // - **`tui-mouse`** — the TUI tracks the mouse (the Claude CLI does): report the notch as an
  //   SGR wheel event (button 64 up / 65 down), what the desktop wheel sends it.
  // - **`tui-keys`** — the alternate screen (DEC 1049) without mouse tracking, a pager like
  //   `less`: send the arrow key ghostty-web emulates the wheel with there.
  // - **`viewport`** — the normal screen: the emulator's own scrollback is the thing being
  //   scrolled, so move the viewport directly.
  //
  // The two `tui` routes are sent straight to the application rather than replayed as a synthetic
  // `wheel` for the desktop handlers to pick up: dispatching an event and hoping it lands in
  // ghostty-web's canvas listener is plumbing no test can pin, and it silently reached nobody.
  // In a full-screen TUI the pane's own scrollback is the wrong target anyway — repaints fight
  // it, and on the live pane of the history double buffer (`scrollback: 0`) there is none.
  //
  // The route is sampled once, at `touchstart`, so one gesture is never split between routes.
  //
  // Listeners attach as soon as the container exists (not gated on `ready`) so a
  // drag arriving right after the canvas mounts is not missed; `termRef` is read
  // lazily at event time and the handlers no-op until the terminal is available.
  useEffect(() => {
    if (!sessionActive || fixedViewportGrid) {
      logScroll(
        "touch-drag effect skipped sessionActive=%o fixedViewportGrid=%o",
        sessionActive,
        !!fixedViewportGrid,
      );
      return;
    }
    const container = containerRef.current;
    if (!container) {
      logScroll("touch-drag effect: no container yet");
      return;
    }

    let dragging = false;
    let engaged = false;
    let startY = 0;
    let lastY = 0;
    let pixelAccum = 0;
    let moveCount = 0;
    /** Who scrolls, and how it is told — see the routing note above. */
    let route: "tui-mouse" | "tui-keys" | "viewport" = "viewport";

    const cellHeightPx = (): number => {
      const term = termRef.current;
      if (!term || term.rows <= 0) return 0;
      const h = measureTerminalCanvasCssHeight(term);
      return h > 0 ? h / term.rows : 0;
    };

    const onTouchStart = (e: TouchEvent) => {
      moveCount = 0;
      if (e.touches.length !== 1) {
        dragging = false;
        logScroll("touchstart ignored touches=%d (not single finger)", e.touches.length);
        return;
      }
      dragging = true;
      engaged = false;
      pixelAccum = 0;
      startY = e.touches[0].clientY;
      lastY = startY;
      const term = termRef.current;
      const tracking = term?.hasMouseTracking?.() ?? false;
      // Alternate-screen state lives on the wasm terminal, not on the JS `Terminal` — same
      // read as the `isAlternateScreen()` handle method.
      const alternateScreen =
        (term as unknown as { wasmTerm?: { isAlternateScreen?: () => boolean } } | null)?.wasmTerm
          ?.isAlternateScreen?.() ?? false;
      route = tracking ? "tui-mouse" : alternateScreen ? "tui-keys" : "viewport";
      logScroll(
        "touchstart startY=%d cellPx=%.2f rows=%o route=%s tracking=%o altScreen=%o",
        startY,
        cellHeightPx(),
        term?.rows,
        route,
        tracking,
        alternateScreen,
      );
    };

    /** Hand `lines` of finger travel to the application as scroll notches at the touch point —
     *  one per line, so the TUI scrolls with the finger rather than by a wheel's fixed step. */
    const scrollTheTui = (touch: Touch, lines: number) => {
      const notches = Math.min(Math.abs(lines), TOUCH_SCROLL_MAX_NOTCHES_PER_MOVE);
      // Finger down (`lines > 0`) reveals older output — scroll up.
      const up = lines > 0;
      for (let i = 0; i < notches; i++) {
        if (route === "tui-mouse") {
          sendWheelSgrAt(touch.clientX, touch.clientY, up);
        } else {
          onDataRef.current?.(up ? "\x1b[A" : "\x1b[B");
        }
      }
      logMouse("touch-scroll routed to the TUI route=%s notches=%d up=%o", route, notches, up);
    };

    const onTouchMove = (e: TouchEvent) => {
      moveCount += 1;
      // Only a single active finger scrolls — never during a two-finger pinch.
      if (!dragging || e.touches.length !== 1) {
        logScroll("touchmove#%d bailed dragging=%o touches=%d", moveCount, dragging, e.touches.length);
        return;
      }
      const term = termRef.current;
      if (!term) {
        logScroll("touchmove#%d bailed: term not ready", moveCount);
        return;
      }
      const touch = e.touches[0];
      const y = touch.clientY;
      const fromStart = y - startY;
      // A tap (negligible movement) must not scroll — wait for real drag distance.
      if (!engaged && Math.abs(fromStart) < TOUCH_SCROLL_START_THRESHOLD_PX) {
        logScroll("touchmove#%d below threshold fromStart=%.1f cancelable=%o", moveCount, fromStart, e.cancelable);
        return;
      }
      engaged = true;
      const cell = cellHeightPx();
      if (cell <= 0) {
        logScroll("touchmove#%d bailed: cellPx=%.2f (canvas not measured)", moveCount, cell);
        return;
      }
      pixelAccum += y - lastY;
      lastY = y;
      const lines = Math.trunc(pixelAccum / cell);
      const vyBefore = term.getViewportY?.();
      if (lines !== 0) {
        pixelAccum -= lines * cell;
        if (route !== "viewport") {
          // A full-screen application owns its own scrolling — give it the gesture.
          scrollTheTui(touch, lines);
        } else {
          // Finger down (positive delta) reveals older output. `scrollLines` moves the
          // viewport by `viewportY - amount`, so a negative amount increases getViewportY
          // (scrolls back toward earlier output) — a natural, content-following drag.
          term.scrollLines(-lines);
          logMouse("touch-scroll lines=%d viewportY=%d", -lines, term.getViewportY?.());
        }
      }
      logScroll(
        "touchmove#%d y=%d fromStart=%.1f cellPx=%.2f accum=%.1f lines=%d vy=%o->%o cancelable=%o",
        moveCount,
        y,
        fromStart,
        cell,
        pixelAccum,
        lines,
        vyBefore,
        term.getViewportY?.(),
        e.cancelable,
      );
      // Non-passive listener: stop the page from scrolling while dragging the terminal.
      // If the browser already claimed the gesture, the event is non-cancelable and this is a no-op.
      if (e.cancelable) e.preventDefault();
    };

    const endDrag = (e: TouchEvent) => {
      logScroll("%s dragging=%o engaged=%o moves=%d", e.type, dragging, engaged, moveCount);
      dragging = false;
      engaged = false;
      pixelAccum = 0;
    };

    container.addEventListener("touchstart", onTouchStart, { passive: true });
    container.addEventListener("touchmove", onTouchMove, { passive: false });
    container.addEventListener("touchend", endDrag, { passive: true });
    container.addEventListener("touchcancel", endDrag, { passive: true });

    // `touch-action` decides whether the browser hands touchmove to us or pans the page
    // itself (and cancels our events). If it isn't `none`/`pan-*`, real-device drags
    // never reach the handler even though synthetic test events do.
    const touchAction =
      typeof window !== "undefined" && window.getComputedStyle
        ? window.getComputedStyle(container).touchAction
        : "(unknown)";
    logScroll(
      "touch-drag listeners attached touchAction=%s containerSize=%dx%d",
      touchAction,
      container.clientWidth,
      container.clientHeight,
    );

    return () => {
      container.removeEventListener("touchstart", onTouchStart);
      container.removeEventListener("touchmove", onTouchMove);
      container.removeEventListener("touchend", endDrag);
      container.removeEventListener("touchcancel", endDrag);
    };
  }, [sessionActive, fixedViewportGrid]);
}
