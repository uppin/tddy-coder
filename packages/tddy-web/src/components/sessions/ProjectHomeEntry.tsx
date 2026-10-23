import React, { useState } from "react";
import type { SessionEntry } from "../../gen/session_pb";
import { cn } from "../../lib/utils";
import { SessionIndicatorDot } from "./SessionIndicatorDot";
import type { ProjectHomeInfo } from "../../utils/projectHomeGroups";

/**
 * The pinned **main** entry first in a project group (`#project-home` 4/7): label `main`, the
 * project's main branch ref, the home agent's label, and the home session's status dot once it
 * exists. Clicking it ensures the project-home session; a refusal is shown on the entry. With no
 * home agent it shows a hint pointing at the Projects screen and starts nothing.
 *
 * Feature doc: docs/ft/web/session-drawer.md § Pinned main entry.
 */
export function ProjectHomeEntry(props: {
  home: ProjectHomeInfo;
  homeSession: SessionEntry | null;
  selected: boolean;
  onEnsure: (projectId: string) => Promise<void>;
}): React.ReactElement | null {
  const { home, homeSession, selected, onEnsure } = props;
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const hasHomeAgent = home.homeAgentLabel !== null;

  const activate = () => {
    if (!hasHomeAgent || pending) return;
    setError(null);
    setPending(true);
    onEnsure(home.projectId)
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setPending(false));
  };

  return (
    <div>
      <button
        type="button"
        data-testid={`sessions-drawer-project-home-${home.projectId}`}
        aria-current={selected ? "true" : undefined}
        aria-disabled={!hasHomeAgent}
        onClick={activate}
        className={cn(
          "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm transition-colors hover:bg-muted",
          selected && "bg-muted",
          !hasHomeAgent && "text-muted-foreground",
        )}
      >
        {homeSession && <SessionIndicatorDot session={homeSession} sizeClassName="h-2 w-2" />}
        <span className="font-medium">main</span>
        <span className="truncate text-xs text-muted-foreground">{home.mainBranchRef}</span>
        {home.homeAgentLabel && (
          <span className="ml-auto truncate text-xs text-muted-foreground">
            {home.homeAgentLabel}
          </span>
        )}
      </button>
      {!hasHomeAgent && (
        <div
          data-testid={`sessions-drawer-project-home-hint-${home.projectId}`}
          className="px-2 pb-1 text-xs text-muted-foreground"
        >
          Set a home agent for this project on the Projects screen to start its main session.
        </div>
      )}
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
