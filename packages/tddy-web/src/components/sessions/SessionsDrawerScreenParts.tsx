import { PanelLeftOpen } from "lucide-react";
import { Button } from "../ui/button";

/** The mobile floating button that re-opens the session list once it has been closed. */
export function SessionsDrawerOpenListButton({ onOpen }: { onOpen: () => void }) {
  return (
    <button
      type="button"
      data-testid="sessions-drawer-open-overlay-btn"
      onClick={onOpen}
      title="Open session list"
      className="absolute top-2 left-2 z-20 flex items-center justify-center h-9 w-9 rounded-md border border-border bg-background/90 text-foreground shadow-md backdrop-blur-sm hover:bg-muted transition-colors"
    >
      <PanelLeftOpen className="h-5 w-5" />
    </button>
  );
}

/** The detail pane's not-found state for a deep link naming no known session. */
export function SessionsDrawerUnknownSession({ onHome }: { onHome: () => void }) {
  return (
    <div className="flex flex-1 min-h-0 items-center justify-center p-6">
      <div
        data-testid="terminal-route-unknown-session"
        className="rounded-md border border-destructive/40 bg-destructive/5 p-4"
      >
        <p className="mb-3 text-sm text-foreground">
          Session not found or no longer available.
        </p>
        <Button
          type="button"
          variant="secondary"
          data-testid="terminal-route-unknown-session-home"
          onClick={onHome}
        >
          Back to sessions
        </Button>
      </div>
    </div>
  );
}
