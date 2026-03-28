/**
 * Overlay Terminate + remote session ended helpers for GhosttyTerminalLiveKit.
 */

/**
 * Invoked when the user clicks Terminate (SIGINT). Parent performs `signalSession(..., SIGINT)`.
 */
export function handleTerminateOverlayClick(onTerminate: (() => void) | undefined): void {
  console.debug("[ghosttyTerminalSessionHandlers] Terminate overlay click");
  onTerminate?.();
}

/**
 * Invoked when the remote coder/server session ends (e.g. server participant disconnected).
 * Parent should clear connected state (return to Connection screen).
 */
export function notifyRemoteSessionEnded(onRemoteSessionEnded: (() => void) | undefined): void {
  console.info("[ghosttyTerminalSessionHandlers] remote session ended; notifying parent");
  onRemoteSessionEnded?.();
}
