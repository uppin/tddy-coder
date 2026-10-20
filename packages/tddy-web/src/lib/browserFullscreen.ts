import { useCallback, useEffect, useState, type RefObject } from "react";
import { emitTddyMarker } from "./tddyMarker";

type FullscreenElement = Element & {
  requestFullscreen?: () => Promise<void>;
  webkitRequestFullscreen?: () => void;
  mozRequestFullScreen?: () => void;
  msRequestFullscreen?: () => void;
};

type FullscreenDocument = Document & {
  webkitExitFullscreen?: () => void;
  mozCancelFullScreen?: () => void;
  msExitFullscreen?: () => void;
};

function logDebug(...args: unknown[]): void {
  console.debug("[tddy][browserFullscreen]", ...args);
}

function logInfo(...args: unknown[]): void {
  console.info("[tddy][browserFullscreen]", ...args);
}

/**
 * Enter browser fullscreen for `target` (standard + vendor-prefixed APIs) — a connected terminal
 * subtree, a remote desktop viewer, any element that should take the whole screen.
 */
export async function requestElementFullscreen(
  target: Element | null,
): Promise<void> {
  emitTddyMarker("M001", "browserFullscreen::requestElementFullscreen", {
    hasTarget: target !== null,
  });
  logDebug("requestElementFullscreen", { hasTarget: target !== null });
  if (!target) {
    logInfo("requestElementFullscreen: missing target, skipping");
    return;
  }
  const el = target as FullscreenElement;
  try {
    if (typeof el.requestFullscreen === "function") {
      await el.requestFullscreen();
      logInfo("requestElementFullscreen: entered via requestFullscreen");
      return;
    }
    if (typeof el.webkitRequestFullscreen === "function") {
      el.webkitRequestFullscreen();
      logInfo("requestElementFullscreen: entered via webkitRequestFullscreen");
      return;
    }
    if (typeof el.mozRequestFullScreen === "function") {
      el.mozRequestFullScreen();
      logInfo("requestElementFullscreen: entered via mozRequestFullScreen");
      return;
    }
    if (typeof el.msRequestFullscreen === "function") {
      el.msRequestFullscreen();
      logInfo("requestElementFullscreen: entered via msRequestFullscreen");
      return;
    }
    logInfo("requestElementFullscreen: no supported fullscreen API on element");
  } catch (e) {
    logInfo("requestElementFullscreen: request failed", e);
    throw e;
  }
}

/**
 * Exit document fullscreen (standard + vendor-prefixed).
 */
export async function exitDocumentFullscreen(): Promise<void> {
  emitTddyMarker("M001b", "browserFullscreen::exitDocumentFullscreen", {});
  logDebug("exitDocumentFullscreen");
  if (!document.fullscreenElement) {
    logInfo("exitDocumentFullscreen: no fullscreen element, skipping");
    return;
  }
  const d = document as FullscreenDocument;
  try {
    if (typeof document.exitFullscreen === "function") {
      await document.exitFullscreen();
      logInfo("exitDocumentFullscreen: exited via exitFullscreen");
      return;
    }
    if (typeof d.webkitExitFullscreen === "function") {
      d.webkitExitFullscreen();
      logInfo("exitDocumentFullscreen: exited via webkitExitFullscreen");
      return;
    }
    if (typeof d.mozCancelFullScreen === "function") {
      d.mozCancelFullScreen();
      logInfo("exitDocumentFullscreen: exited via mozCancelFullScreen");
      return;
    }
    if (typeof d.msExitFullscreen === "function") {
      d.msExitFullscreen();
      logInfo("exitDocumentFullscreen: exited via msExitFullscreen");
    }
  } catch (e) {
    logInfo("exitDocumentFullscreen: exit failed", e);
    throw e;
  }
}

/** Whether `target` is the current fullscreen element (or contains it). */
export function isTargetInActiveFullscreen(target: Element | null): boolean {
  if (!target) return false;
  const active = document.fullscreenElement;
  if (!active) return false;
  return active === target || target.contains(active);
}

/** Full-screen state of one element, and the two ways to change it. */
export interface ElementFullscreen {
  /** Whether the element behind `ref` is (or contains) the document's fullscreen element. */
  isFullscreen: boolean;
  /** Hand the element behind `ref` to the Fullscreen API. */
  enter(): Promise<void>;
  /** Leave document fullscreen. */
  exit(): Promise<void>;
}

/**
 * Track whether the element behind `ref` is full screen, following `fullscreenchange` — so a leave
 * the browser performed on its own (Escape, a system gesture) is reported as well as one asked for.
 */
export function useElementFullscreen(ref: RefObject<HTMLElement | null>): ElementFullscreen {
  const [isFullscreen, setIsFullscreen] = useState(false);

  useEffect(() => {
    const onFullscreenChange = () => {
      setIsFullscreen(isTargetInActiveFullscreen(ref.current));
    };
    document.addEventListener("fullscreenchange", onFullscreenChange);
    return () => {
      document.removeEventListener("fullscreenchange", onFullscreenChange);
    };
  }, [ref]);

  const enter = useCallback(() => requestElementFullscreen(ref.current), [ref]);
  const exit = useCallback(() => exitDocumentFullscreen(), []);

  return { isFullscreen, enter, exit };
}
