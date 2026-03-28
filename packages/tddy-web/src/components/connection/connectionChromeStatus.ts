/**
 * Maps LiveKit overlay status to `data-connection-status` (DOM attribute value).
 */
export function dataConnectionStatusValue(
  status: "connecting" | "connected" | "error",
): string {
  return status;
}
