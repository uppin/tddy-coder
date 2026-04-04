/**
 * Terminal presentation state for reconnect overlay / mini / full (PRD: terminal-reconnect-overlay).
 *
 * Tunable overlay dimensions: `config.ts`.
 */

import {
  TERMINAL_OVERLAY_PANE_MIN_HEIGHT_PX,
  TERMINAL_OVERLAY_PANE_MIN_WIDTH_PX,
} from "./config";

export {
  TERMINAL_OVERLAY_COLS,
  TERMINAL_OVERLAY_FONT_MIN_PX,
  TERMINAL_OVERLAY_PANE_HEIGHT_PX,
  TERMINAL_OVERLAY_PANE_MAX_DEFAULT_WIDTH_PX,
  TERMINAL_OVERLAY_PANE_MIN_HEIGHT_PX,
  TERMINAL_OVERLAY_PANE_MIN_WIDTH_PX,
  TERMINAL_OVERLAY_PANE_WIDTH_PX,
  TERMINAL_OVERLAY_ROWS,
} from "./config";

/**
 * Clamp overlay terminal pane size for drag resize (min defaults, caller supplies viewport max).
 */
export function clampTerminalOverlayPaneSize(
  width: number,
  height: number,
  maxWidth: number,
  maxHeight: number,
): { width: number; height: number } {
  const effMaxW = Math.max(TERMINAL_OVERLAY_PANE_MIN_WIDTH_PX, maxWidth);
  const effMaxH = Math.max(TERMINAL_OVERLAY_PANE_MIN_HEIGHT_PX, maxHeight);
  const w = Math.min(
    effMaxW,
    Math.max(TERMINAL_OVERLAY_PANE_MIN_WIDTH_PX, Math.round(width)),
  );
  const h = Math.min(
    effMaxH,
    Math.max(TERMINAL_OVERLAY_PANE_MIN_HEIGHT_PX, Math.round(height)),
  );
  return { width: w, height: h };
}

export type TerminalPresentation = "hidden" | "overlay" | "mini" | "full";

export type TerminalAttachKind = "new" | "reconnect";

export type SessionControlAction = "startSession" | "connectSession" | "resumeSession";

export type TransitionCounters = {
  connectSessionCalls: number;
  resumeSessionCalls: number;
  disconnectCalls: number;
};

function tpDebug(message: string, data?: Record<string, unknown>): void {
  if (data !== undefined) {
    console.debug(`[tddy][terminalPresentation] ${message}`, data);
  } else {
    console.debug(`[tddy][terminalPresentation] ${message}`);
  }
}

/** Maps ConnectionScreen handlers to attach semantics (new vs reconnect). */
export function attachKindForSessionControl(action: SessionControlAction): TerminalAttachKind {
  if (action === "resumeSession") {
    tpDebug("attachKindForSessionControl: resumeSession → reconnect");
    return "reconnect";
  }
  tpDebug(`attachKindForSessionControl: ${action} → new`);
  return "new";
}

/** Presentation + whether to push /terminal/:id after attach. */
export function nextPresentationFromAttach(
  prev: TerminalPresentation,
  kind: TerminalAttachKind,
): { presentation: TerminalPresentation; shouldPushTerminalRoute: boolean } {
  if (kind === "new") {
    tpDebug("nextPresentationFromAttach: new attach", {
      prev,
      presentation: "overlay",
      shouldPushTerminalRoute: false,
    });
    return { presentation: "overlay", shouldPushTerminalRoute: false };
  }
  tpDebug("nextPresentationFromAttach: reconnect attach", { prev, presentation: "overlay" });
  return { presentation: "overlay", shouldPushTerminalRoute: false };
}

/** Repeated reconnect signals must not stack multiple overlay instances (show-at-most-once). */
export function reconcileReconnectOverlayInstances(reconnectSignalCount: number): number {
  const logical = reconnectSignalCount > 0 ? 1 : 0;
  tpDebug("reconcileReconnectOverlayInstances", {
    reconnectSignalCount,
    logical,
  });
  return logical;
}

/** User clicked overlay preview to go full — must not invoke another connect for the same session. */
export function applyOverlayPreviewClickToFull(counters: TransitionCounters): {
  presentation: TerminalPresentation;
  counters: TransitionCounters;
} {
  tpDebug("applyOverlayPreviewClickToFull: overlay → full (counters unchanged)");
  return {
    presentation: "full",
    counters: { ...counters },
  };
}

/** Dedicated terminal Back → mini without disconnecting LiveKit. */
export function applyDedicatedTerminalBackToMini(counters: TransitionCounters): {
  presentation: TerminalPresentation;
  counters: TransitionCounters;
} {
  tpDebug("applyDedicatedTerminalBackToMini: full → mini (disconnect unchanged)");
  return {
    presentation: "mini",
    counters: { ...counters },
  };
}

/** Floating mini/overlay default placement (PRD: bottom-right). */
export function defaultTerminalMiniOverlayPlacement(): "bottom-left" | "bottom-right" | "top-left" | "top-right" {
  tpDebug("defaultTerminalMiniOverlayPlacement: bottom-right");
  return "bottom-right";
}
