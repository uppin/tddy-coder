/**
 * Terminal presentation state for reconnect overlay / mini / full (PRD: terminal-reconnect-overlay).
 */

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
    const shouldPushTerminalRoute = prev !== "full";
    tpDebug("nextPresentationFromAttach: new attach", {
      prev,
      presentation: "full",
      shouldPushTerminalRoute,
    });
    return { presentation: "full", shouldPushTerminalRoute };
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
