import { paneCoverStyle } from "./paneStyles";

/**
 * Covers over the terminal panes: the history loading indicator, the connection error and the
 * session-ended notice.
 */
export function PaneOverlays({
  loading,
  showError,
  error,
  sessionEnded,
}: {
  loading: boolean;
  showError: boolean;
  error: string | undefined;
  sessionEnded: boolean;
}) {
  return (
    <>
      {/* Loading indicator: shown over whichever pane is foreground while the background page
          terminal is being forward-filled. */}
      {loading ? (
        <div
          data-testid="terminal-history-loading"
          style={{
            position: "absolute",
            top: 6,
            left: "50%",
            transform: "translateX(-50%)",
            zIndex: 6,
            fontSize: 11,
            padding: "2px 10px",
            background: "var(--muted, #2a2b3d)",
            color: "var(--muted-foreground, #a9b1d6)",
            border: "1px solid var(--border, #333)",
            borderRadius: 4,
            pointerEvents: "none",
          }}
        >
          Loading history…
        </div>
      ) : null}
      {showError ? (
        <div data-testid="livekit-error" role="alert" style={{ ...paneCoverStyle, zIndex: 48 }}>
          {error}
        </div>
      ) : null}
      {sessionEnded && (
        <div
          data-testid="terminal-coder-unavailable"
          role="status"
          style={{ ...paneCoverStyle, zIndex: 50 }}
        >
          Session ended — the coder disconnected. Reconnect from the session list to continue.
        </div>
      )}
    </>
  );
}
