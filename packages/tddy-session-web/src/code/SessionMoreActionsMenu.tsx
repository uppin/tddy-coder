import { useState } from "react";

export type SessionMoreActionsMenuProps = {
  sessionId: string;
  /** Invoked when the user chooses **Show files** (after the menu closes). */
  onShowFiles: () => void;
};

/**
 * Per-session overflow menu with **Show files** for workflow artifacts (opens session files UI from parent).
 */
export function SessionMoreActionsMenu({ sessionId, onShowFiles }: SessionMoreActionsMenuProps) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative inline-block text-left">
      <button
        type="button"
        data-testid={`session-more-actions-${sessionId}`}
        onClick={() => setOpen(!open)}
        aria-haspopup="menu"
        aria-expanded={open}
        className="rounded-md border border-input bg-background px-2 py-1 text-xs shadow-sm hover:bg-muted"
      >
        More actions
      </button>
      {open ? (
        <ul
          role="menu"
          data-testid={`session-more-actions-menu-${sessionId}`}
          className="absolute right-0 z-20 mt-1 min-w-[10rem] rounded-md border border-border bg-popover py-1 text-popover-foreground shadow-md"
        >
          <li role="none">
            <button
              type="button"
              role="menuitem"
              data-testid="session-more-actions-show-files"
              className="block w-full px-3 py-2 text-left text-sm hover:bg-muted"
              onClick={() => {
                setOpen(false);
                onShowFiles();
              }}
            >
              Show files
            </button>
          </li>
        </ul>
      ) : null}
    </div>
  );
}
