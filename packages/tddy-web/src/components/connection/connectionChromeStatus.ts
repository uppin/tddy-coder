import { emitConnectionChromeMarker } from "./connectionChromeMarkers";

/**
 * Maps LiveKit overlay status to `data-connection-status` (DOM attribute value).
 */
export function dataConnectionStatusValue(
  status: "connecting" | "connected" | "error",
): string {
  emitConnectionChromeMarker("M006", "connectionChromeStatus:dataConnectionStatusValue", {
    status,
  });
  console.debug("[connectionChromeStatus] data-connection-status value", status);
  return status;
}
