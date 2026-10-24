import React, { useState } from "react";
import { Replace } from "lucide-react";
import type { SessionEntry } from "../../gen/session_pb";
import { cn } from "../../lib/utils";
import { SessionIndicatorDot } from "./SessionIndicatorDot";
import type { ProjectHomeInfo } from "../../utils/projectHomeGroups";

/**
 * The pinned **main** entry first in a project group (`#project-home` 4/7): label `main`, the
 * project's main branch ref, main's model, and main's status dot once it is listed. Clicking it
 * ensures the session the project records as its main; a refusal is shown on the entry. With no
 * main set up it starts nothing: a click opens the setup pane for that project
 * (`onSetUpProjectHome`), and without that handler the entry is disabled. A set-up main also offers
 * **Replace main** (`onReplaceProjectHome`), which opens the setup pane in replace mode.
 *
 * Feature doc: docs/ft/web/session-drawer.md § Pinned main entry.
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md
 */
export function ProjectHomeEntry(props: {
  home: ProjectHomeInfo;
  homeSession: SessionEntry | null;
  selected: boolean;
  onEnsure: (projectId: string) => Promise<void>;
  /** Called instead of `onEnsure` when the project has no main to reach. */
  onSetUpProjectHome?: (projectId: string) => void;
  /** Set main up again for a project whose main is set up; absent: no Replace main action. */
  onReplaceProjectHome?: (projectId: string) => void;
}): React.ReactElement | null {
  const { home, homeSession, selected, onEnsure, onSetUpProjectHome, onReplaceProjectHome } = props;
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const hasMain = home.homeSessionId !== null;

  const activate = () => {
    if (!hasMain) {
      onSetUpProjectHome?.(home.projectId);
      return;
    }
    if (pending) return;
    setError(null);
    setPending(true);
    onEnsure(home.projectId)
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setPending(false));
  };

  return (
    <div>
      <div className="flex items-center">
        <button
          type="button"
          data-testid={`sessions-drawer-project-home-${home.projectId}`}
          aria-current={selected ? "true" : undefined}
          aria-disabled={!hasMain && !onSetUpProjectHome}
          onClick={activate}
          className={cn(
            "flex min-w-0 flex-1 items-center gap-2 rounded px-2 py-1.5 text-left text-sm transition-colors hover:bg-muted",
            selected && "bg-muted",
            !hasMain && "text-muted-foreground",
          )}
        >
          {homeSession && <SessionIndicatorDot session={homeSession} sizeClassName="h-2 w-2" />}
          <span className="font-medium">main</span>
          <span className="truncate text-xs text-muted-foreground">{home.mainBranchRef}</span>
          {homeSession?.model && (
            <span className="ml-auto truncate text-xs text-muted-foreground">{homeSession.model}</span>
          )}
        </button>
        {hasMain && onReplaceProjectHome && (
          <button
            type="button"
            data-testid={`sessions-drawer-project-home-replace-${home.projectId}`}
            aria-label="Replace main"
            title="Replace main"
            onClick={() => onReplaceProjectHome(home.projectId)}
            className="rounded p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            <Replace className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
      {error !== null && (
        <div
          data-testid={`sessions-drawer-project-home-error-${home.projectId}`}
          role="alert"
          className="px-2 pb-1 text-xs text-destructive"
        >
          {error}
        </div>
      )}
    </div>
  );
}
