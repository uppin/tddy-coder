import { useEffect, type RefObject } from "react";
import type { TerminalViewHandle } from "../terminal/types";

/** Drives the history and mouse-tracking paths from component tests. */
export function useTestWindowHooks(
  termRef: RefObject<TerminalViewHandle | null>,
  olderTermRef: RefObject<TerminalViewHandle | null>,
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
