import { emitTddyMarker } from "./tddyMarker";

export type LiveKitChromeStatus = "connecting" | "connected" | "error";

function logDebug(...args: unknown[]): void {
  console.debug("[tddy][liveKitStatusPresentation]", ...args);
}

/**
 * Whether the raw `livekit-status` strip should occupy visible layout when connection chrome is shown.
 * When the overlay is on, the connection dot conveys state; errors use `livekit-error` / other UI.
 */
export function shouldShowVisibleLiveKitStatusStrip(args: {
  connectionOverlayEnabled: boolean;
  status: LiveKitChromeStatus;
}): boolean {
  emitTddyMarker("M003", "liveKitStatusPresentation::shouldShowVisibleLiveKitStatusStrip", {
    connectionOverlayEnabled: args.connectionOverlayEnabled,
    status: args.status,
  });
  logDebug("shouldShowVisibleLiveKitStatusStrip", args);
  if (args.connectionOverlayEnabled) {
    return false;
  }
  return true;
}
