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
 * Enter browser fullscreen for the connected terminal subtree (standard + vendor-prefixed APIs).
 */
export async function requestFullscreenForConnectedTerminal(
  target: Element | null,
): Promise<void> {
  emitTddyMarker("M001", "browserFullscreen::requestFullscreenForConnectedTerminal", {
    hasTarget: target !== null,
  });
  logDebug("requestFullscreenForConnectedTerminal", { hasTarget: target !== null });
  if (!target) {
    logInfo("requestFullscreenForConnectedTerminal: missing target, skipping");
    return;
  }
  const el = target as FullscreenElement;
  try {
    if (typeof el.requestFullscreen === "function") {
      await el.requestFullscreen();
      logInfo("requestFullscreenForConnectedTerminal: entered via requestFullscreen");
      return;
    }
    if (typeof el.webkitRequestFullscreen === "function") {
      el.webkitRequestFullscreen();
      logInfo("requestFullscreenForConnectedTerminal: entered via webkitRequestFullscreen");
      return;
    }
    if (typeof el.mozRequestFullScreen === "function") {
      el.mozRequestFullScreen();
      logInfo("requestFullscreenForConnectedTerminal: entered via mozRequestFullScreen");
      return;
    }
    if (typeof el.msRequestFullscreen === "function") {
      el.msRequestFullscreen();
      logInfo("requestFullscreenForConnectedTerminal: entered via msRequestFullscreen");
      return;
    }
    logInfo("requestFullscreenForConnectedTerminal: no supported fullscreen API on element");
  } catch (e) {
    logInfo("requestFullscreenForConnectedTerminal: request failed", e);
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
