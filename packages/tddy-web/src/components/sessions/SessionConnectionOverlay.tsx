import type { ConnectionStatus } from "../../rpc/connections/types";

export interface SessionConnectionOverlayProps {
  /** The session connection's own status — see `SessionConnection.status`. */
  status: ConnectionStatus;
}

/**
 * Connection overlay for a session runtime's panes — rendered over the pane stack while the
 * session's connection is still coming up, and kept up with an error message if it fails. Pure
 * presentation, driven by a single `status` prop; the status is the session connection's own, so
 * every wire gets one (it used to be the LiveKit room handshake's, which is why a session its host
 * served directly showed no connection state at all). Once the connection is up the overlay renders
 * nothing, so the panes become interactive.
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
