/** How the connection carrying a terminal feed is doing, as the terminal chrome reports it. */
export type TerminalConnectionStatus = "connecting" | "connected" | "error";

/**
 * Whether the raw connection-status strip should occupy visible layout when connection chrome is
 * shown. When the overlay is on, the connection dot conveys state; errors use other UI.
 */
export function shouldShowVisibleTerminalStatusStrip(args: {
  connectionOverlayEnabled: boolean;
  status: TerminalConnectionStatus;
}): boolean {
  if (args.connectionOverlayEnabled) {
    return false;
  }
  return true;
}
