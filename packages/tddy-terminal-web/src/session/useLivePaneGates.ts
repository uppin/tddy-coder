import { useEffect, type RefObject } from "react";
import type { GhosttyTerminalHandle } from "../terminal/types";
import { dMouse } from "./sessionDiagnostics";

/**
 * The wheel gate on the live pane. See the note on the effect.
 */
export function useLiveWheelGate(
  liveContainerRef: RefObject<HTMLDivElement | null>,
  termRef: RefObject<GhosttyTerminalHandle | null>,
): void {
  // Wheel gate on the live pane (capture phase — runs before ghostty-web's canvas handler):
  // 1. Mouse tracking ON → SGR wheel report to the TUI; block ghostty-web arrow emulation.
  // 2. Mouse tracking OFF → no-op here; ghostty-web emits Up/Down for pagers on the alternate screen.
  // The wheel never swaps panes: switching to and from history is a manual toggle (the affordances
  // below), so scrolling inside a TUI can't drop the user out of the live view by accident.
  // Installed whether or not the connection serves history: routing the wheel to a mouse-tracking
  // TUI has nothing to do with scrollback, and without the gate ghostty-web emulates arrow keys.
  useEffect(() => {
    const el = liveContainerRef.current;
    if (!el) return;
    const handler = (e: WheelEvent) => {
      const tracking = termRef.current?.hasMouseTracking?.() ?? false;
      dMouse(
        "wheel gate tracking=%o altScreen=%o ctrl=%o deltaY=%d",
        tracking,
        termRef.current?.isAlternateScreen?.() ?? false,
        e.ctrlKey,
        e.deltaY,
      );
      if (!tracking) return;
      if (!e.ctrlKey) {
        termRef.current?.sendWheelSgr?.(e);
      }
      e.preventDefault();
      e.stopPropagation();
    };
    el.addEventListener("wheel", handler, { capture: true });
    return () => el.removeEventListener("wheel", handler, { capture: true } as AddEventListenerOptions);
  }, []);
}

/** Drives the history and mouse-tracking paths from component tests. */
export function useTestWindowHooks(
  termRef: RefObject<GhosttyTerminalHandle | null>,
  olderTermRef: RefObject<GhosttyTerminalHandle | null>,
): void {
  // FIXME: test-only hooks; remove if real wheel/keystroke drivers become available in Cypress.
  useEffect(() => {
    const win = window as unknown as {
      __tddyPageScrollUp?: (n: number) => void;
      __tddyPageScrollToLine?: (n: number) => void;
      __tddyLiveMouseTracking?: (on: boolean) => void;
    };
    win.__tddyPageScrollUp = (n: number) => {
      olderTermRef.current?.scrollLines?.(-n);
    };
    win.__tddyPageScrollToLine = (n: number) => {
      olderTermRef.current?.scrollToLine?.(n);
    };
    win.__tddyLiveMouseTracking = (on: boolean) => {
      if (on) {
        termRef.current?.write("\x1b[?1006h\x1b[?1002h");
      } else {
        termRef.current?.write("\x1b[?1002l\x1b[?1006l");
      }
    };
    return () => {
      delete win.__tddyPageScrollUp;
      delete win.__tddyPageScrollToLine;
      delete win.__tddyLiveMouseTracking;
    };
  }, []);
}
