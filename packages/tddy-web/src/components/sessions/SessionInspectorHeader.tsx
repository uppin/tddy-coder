import type { ReactNode } from "react";
import { Button } from "../ui/button";
import type { InspectorDrawerState } from "./SessionInspectorDrawer";

interface SessionInspectorHeaderProps {
  state: InspectorDrawerState;
  onClose: () => void;
  onExpand: () => void;
  onRestore: () => void;
}

/** A 14px stroke icon in the header's button style. */
function HeaderIcon({ children }: { children: ReactNode }) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {children}
    </svg>
  );
}

/** The inspector's title row: Expand (while open), Restore (while expanded), and Close. */
export function SessionInspectorHeader({ state, onClose, onExpand, onRestore }: SessionInspectorHeaderProps) {
  return (
    <div className="flex items-center justify-between px-3 py-2 border-b border-border flex-shrink-0">
      <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
        Inspector
      </span>
      <div className="flex items-center gap-1">
        {state === "open" && (
          <Button
            data-testid="sessions-inspector-expand"
            variant="ghost"
            size="sm"
            className="h-6 w-6 p-0"
            onClick={onExpand}
            title="Expand"
          >
            <HeaderIcon>
              <polyline points="15 3 21 3 21 9" />
              <polyline points="9 21 3 21 3 15" />
              <line x1="21" y1="3" x2="14" y2="10" />
              <line x1="3" y1="21" x2="10" y2="14" />
            </HeaderIcon>
          </Button>
        )}
        {state === "expanded" && (
          <Button
            data-testid="sessions-inspector-restore"
            variant="ghost"
            size="sm"
            className="h-6 w-6 p-0"
            onClick={onRestore}
            title="Restore"
          >
            <HeaderIcon>
              <polyline points="4 14 10 14 10 20" />
              <polyline points="20 10 14 10 14 4" />
              <line x1="10" y1="14" x2="3" y2="21" />
              <line x1="21" y1="3" x2="14" y2="10" />
            </HeaderIcon>
          </Button>
        )}
        <Button
          data-testid="sessions-inspector-close"
          variant="ghost"
          size="sm"
          className="h-6 w-6 p-0"
          onClick={onClose}
          title="Close"
        >
          <HeaderIcon>
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </HeaderIcon>
        </Button>
      </div>
    </div>
  );
}
