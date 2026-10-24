import React from "react";
import { Loader2, RefreshCw } from "lucide-react";
import { formatDiskBytes } from "../sessions/worktreeStatsFormat";
import { sizingProgress, type WorktreeListItem } from "../../lib/worktreeList";
import { cn } from "../../lib/utils";
import { Button } from "../ui/button";
import { ScrollArea } from "../ui/scroll-area";

export interface WorktreesScreenProps {
  projects: ReadonlyArray<{ projectId: string; name: string }>;
  projectId: string;
  onSelectProject: (projectId: string) => void;
  /** Already sorted, biggest first. */
  items: readonly WorktreeListItem[];
  selectedPath: string | null;
  onSelect: (path: string) => void;
  /** Recalculate every worktree's size. */
  onRefresh: () => void;
  /** The selected worktree's detail pane. */
  children?: React.ReactNode;
  /** Shown in the list when it is empty. */
  emptyHint: string;
}

const selectClassName =
  "box-border w-full rounded-md border border-input bg-background px-2 py-1.5 text-sm text-foreground shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

/**
 * The Worktrees screen in the Sessions screen's layout: a side panel listing the project's
 * worktrees by name with their disk usage (project selector and Refresh above the list), and the
 * selected worktree's pane beside it.
 */
export function WorktreesScreen({
  projects,
  projectId,
  onSelectProject,
  items,
  selectedPath,
  onSelect,
  onRefresh,
  children,
  emptyHint,
}: WorktreesScreenProps) {
  const { done, total } = sizingProgress(items);
  const calculating = done < total;

  return (
    <div data-testid="worktrees-screen" className="flex flex-1 min-h-0 overflow-hidden flex-col md:flex-row">
      <div
        data-testid="worktrees-sidebar"
        className="flex flex-col border-b md:border-b-0 md:border-r border-border bg-background flex-shrink-0 md:w-[280px] max-h-[45dvh] md:max-h-none md:h-full"
      >
        <div className="px-3 py-2 border-b border-border flex flex-col gap-2">
          <div className="flex items-center gap-1">
            <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground flex-1">
              Worktrees
            </span>
            <Button
              type="button"
              size="xs"
              variant="outline"
              data-testid="worktrees-refresh"
              disabled={projectId === ""}
              onClick={onRefresh}
            >
              <RefreshCw className={cn("h-3 w-3", calculating && "animate-spin")} />
              Refresh
            </Button>
          </div>
          <select
            aria-label="Project"
            data-testid="worktrees-project-select"
            className={selectClassName}
            value={projectId}
            onChange={(e) => onSelectProject(e.target.value)}
          >
            <option value="">—</option>
            {projects.map((p) => (
              <option key={p.projectId} value={p.projectId}>
                {p.name || p.projectId}
              </option>
            ))}
          </select>
          {calculating ? (
            <div data-testid="worktrees-progress" className="flex flex-col gap-1">
              <span className="text-xs text-muted-foreground">
                Calculating disk usage… {done} of {total}
              </span>
              <div className="h-1 w-full overflow-hidden rounded bg-muted">
                <div
                  className="h-full bg-primary transition-[width]"
                  style={{ width: `${total === 0 ? 0 : (done / total) * 100}%` }}
                />
              </div>
            </div>
          ) : null}
        </div>
        <ScrollArea className="flex-1 min-h-0">
          <div className="py-1 px-2 space-y-0.5">
            {items.map((item) => (
              <button
                key={item.path}
                type="button"
                data-testid="worktrees-row"
                title={`${item.path}\n${item.branch}`}
                aria-selected={item.path === selectedPath ? "true" : undefined}
                onClick={() => onSelect(item.path)}
                className={cn(
                  "w-full text-left flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-colors",
                  item.path === selectedPath ? "bg-accent text-accent-foreground" : "hover:bg-muted",
                )}
              >
                <span data-testid="worktrees-row-name" className="truncate flex-1">
                  {item.name}
                </span>
                {item.calculating ? (
                  <Loader2 data-testid="worktrees-row-spinner" className="h-3 w-3 shrink-0 animate-spin text-muted-foreground" />
                ) : null}
                <span
                  data-testid="worktrees-row-size"
                  className={cn(
                    "shrink-0 text-xs tabular-nums text-muted-foreground",
                    item.stale && "italic opacity-70",
                  )}
                >
                  {item.diskBytes !== undefined ? formatDiskBytes(item.diskBytes) : ""}
                </span>
              </button>
            ))}
            {items.length === 0 ? (
              <div className="px-3 py-4 text-sm text-muted-foreground text-center">{emptyHint}</div>
            ) : null}
          </div>
        </ScrollArea>
      </div>
      <div data-testid="worktrees-main" className="flex flex-1 min-w-0 min-h-0 flex-col overflow-hidden">
        {children}
      </div>
    </div>
  );
}
