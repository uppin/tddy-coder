import { emitTddyMarker } from "./tddyMarker";

function logInfo(...args: unknown[]): void {
  console.info("[tddy][remoteTerminateConfirm]", ...args);
}

/**
 * Confirm before terminating the remote session / process (native dialog; same pattern as session delete).
 */
export function confirmRemoteSessionTermination(message: string): boolean {
  emitTddyMarker("M002", "remoteTerminateConfirm::confirmRemoteSessionTermination", {
    messageLength: message.length,
  });
  logInfo("confirmRemoteSessionTermination", { messageLength: message.length });
  const w = globalThis.window;
  if (!w || typeof w.confirm !== "function") {
    logInfo("confirmRemoteSessionTermination: no window.confirm — refusing");
    return false;
  }
  const result = w.confirm(message);
  logInfo("confirmRemoteSessionTermination: user result", { confirmed: result });
  return result;
}
