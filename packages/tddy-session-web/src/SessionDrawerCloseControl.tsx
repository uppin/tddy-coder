import React from "react";
import { Loader2, Trash2, X } from "lucide-react";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import { Button } from "tddy-ui-web";
import { connectionStatusForSession } from "./utils/connectionStatusForSession";

/**
 * A drawer row's close control (docs/ft/web/session-drawer.md § Closing a session): the × on an
 * active row, the mini trash bin on an ended row that still holds a worktree, and the mini spinner
 * in their place while the row's close runs. Nothing on an ended row that holds no worktree.
 */
export function SessionDrawerCloseControl({
  session,
  label,
  closingStep,
  onClose,
}: {
  session: SessionEntry;
  /** The row's drawer label, which the control's accessible name repeats. */
  label: string;
  /** What the row's running close is doing now; `undefined` while no close runs. */
  closingStep: string | undefined;
  onClose: (session: SessionEntry) => void;
}) {
  if (closingStep !== undefined) {
    return (
      <span
        data-testid={`sessions-drawer-item-closing-${session.sessionId}`}
        role="status"
        title={closingStep}
        aria-label={`Closing ${label}: ${closingStep}`}
        className="flex size-6 flex-shrink-0 items-center justify-center text-muted-foreground"
      >
        <Loader2 className="size-3 animate-spin" aria-hidden />
      </span>
    );
  }
  // The same partition the drawer lists rows by: what is not disconnected is active.
  if (connectionStatusForSession(session) !== "disconnected") {
    return (
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        data-testid={`sessions-drawer-item-close-${session.sessionId}`}
        aria-label={`Close session ${label}`}
        title="Close session"
        className="text-muted-foreground"
        onClick={() => onClose(session)}
      >
        <X aria-hidden />
      </Button>
    );
  }
  if (session.holdsWorktree) {
    return (
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        data-testid={`sessions-drawer-item-remove-worktree-${session.sessionId}`}
        aria-label={`Remove worktree of ${label}`}
        title="Remove worktree"
        className="text-muted-foreground"
        onClick={() => onClose(session)}
      >
        <Trash2 aria-hidden />
      </Button>
    );
  }
  return null;
}
