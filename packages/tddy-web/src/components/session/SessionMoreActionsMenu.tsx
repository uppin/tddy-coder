import { useState } from "react";

export type SessionMoreActionsMenuProps = {
  sessionId: string;
};

/**
 * Per-session overflow menu with **Show files** for workflow artifacts (wired to session files UI elsewhere).
 */
export function SessionMoreActionsMenu({ sessionId }: SessionMoreActionsMenuProps) {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button
        type="button"
        data-testid={`session-more-actions-${sessionId}`}
        onClick={() => setOpen(!open)}
        aria-haspopup="menu"
        aria-expanded={open}
      >
        More actions
      </button>
      {open ? (
        <ul role="menu" data-testid={`session-more-actions-menu-${sessionId}`}>
          <li role="none">
            <button
              type="button"
              role="menuitem"
              data-testid="session-more-actions-show-files"
              onClick={() => setOpen(false)}
            >
              Show files
            </button>
          </li>
        </ul>
      ) : null}
    </div>
  );
}
