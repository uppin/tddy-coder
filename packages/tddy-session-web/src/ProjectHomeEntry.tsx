import React, { useState } from "react";
import { Home } from "lucide-react";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import { cn } from "tddy-ui-web";
import { SessionIndicatorDot } from "./SessionIndicatorDot";
import type { ProjectHomeInfo } from "./utils/projectHomeGroups";

/**
 * The pinned **main** entry first in a project group (`#project-home` 4/7): label `main`, the
 * project's main branch ref, main's model, and main's status dot once it is listed. Clicking it
 * ensures the session the project records as its main; a refusal is shown on the entry. With no
 * main set up it starts nothing: a click opens the setup pane for that project
 * (`onSetUpProjectHome`), and without that handler the entry is disabled. **Replace main** lives
 * in the project's settings pane (`#orchestrators` 11/11), not on this entry.
 *
 * Feature doc: docs/ft/web/session-drawer.md § Pinned main entry.
 * PRD: docs/ft/web/session-drawer.md
 */
export function ProjectHomeEntry(props: {
  home: ProjectHomeInfo;
  homeSession: SessionEntry | null;
  selected: boolean;
  onEnsure: (projectId: string) => Promise<void>;
  /** Called instead of `onEnsure` when the project has no main to reach. */
  onSetUpProjectHome?: (projectId: string) => void;
}): React.ReactElement | null {
  const { home, homeSession, selected, onEnsure, onSetUpProjectHome } = props;
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
            // The session rows' box (`SessionDrawerItem`), so main's dot sits in their dot column.
            "flex min-w-0 flex-1 items-center gap-2 rounded-md px-3 py-2 text-left text-sm transition-colors hover:bg-muted",
            selected && "bg-muted",
            !hasMain && "text-muted-foreground",
          )}
        >
          {/* Main's indicator is a house in its session's status colour; muted until it is listed. */}
          {homeSession ? (
            <SessionIndicatorDot session={homeSession} shape="home" />
          ) : (
            <Home
              data-testid={`sessions-drawer-home-icon-project-${home.projectId}`}
              aria-label="Project home"
              className="-mx-0.5 h-3 w-3 flex-shrink-0 text-muted-foreground"
              strokeWidth={2.5}
            />
          )}
          <span className="flex-shrink-0 font-medium">main</span>
          <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">{home.mainBranchRef}</span>
          {homeSession?.model && (
            // Never shrinks to nothing: capped, then truncated, so it stays in view beside the ref.
            <span className="max-w-[50%] flex-shrink-0 truncate text-xs text-muted-foreground">
              {homeSession.model}
            </span>
          )}
        </button>
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

