import type { LiveKitChromeStatus } from "../../lib/liveKitStatusPresentation";

export interface SessionConnectionOverlayProps {
  /** The session's LiveKit connection status, driven by `GhosttyTerminalLiveKit`'s handshake. */
  status: LiveKitChromeStatus;
}

/**
 * Connection overlay for a session runtime's panes — rendered over the pane stack while the
 * session's LiveKit room is still handshaking (token request + room join), and kept up with an
 * error message if that handshake fails. Pure presentation, driven by a single `status` prop; the
 * status is owned by the runtime (see `SessionRuntime` / `SessionLiveKitTerminal`). Once the room is
 * connected the overlay renders nothing, so the panes become interactive.
 *
 * PRD: `docs/ft/web/session-drawer.md` (session connection state).
 */
export function SessionConnectionOverlay({ status }: SessionConnectionOverlayProps) {
  if (status === "connected") return null;

  return (
    <div
      data-testid="session-connection-overlay"
      className="absolute inset-0 z-10 flex flex-col items-center justify-center bg-background/80 backdrop-blur-sm pointer-events-auto"
    >
      {status === "error" ? (
        <p data-testid="session-connection-error" className="text-sm text-destructive">
          Connection failed
        </p>
      ) : (
        <p className="text-sm text-muted-foreground">Connecting…</p>
      )}
    </div>
  );
}
