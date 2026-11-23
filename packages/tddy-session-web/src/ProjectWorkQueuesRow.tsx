import React, { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { ChevronDown, ChevronUp } from "lucide-react";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import {
  WorkQueueService,
  type WorkQueueSummary,
} from "tddy-client-web/gen/work_queue_pb";
import { cn } from "tddy-ui-web";
import { sessionDrawerLabel } from "./utils/sessionDrawerLabel";

/** One project's stacks, as `ListProjectWorkQueues` answered for it. */
export interface ProjectStacks {
  projectId: string;
  projectName: string;
  stacks: WorkQueueSummary[];
}

/**
 * Every listed project's PR stacks on every host (`#orchestrators` 9/11), one
 * `ListProjectWorkQueues` per project, in `projects` order; projects with no stacks are dropped.
 * A project whose listing did not answer counts as having none — no entry is the honest rendering.
 */
export function useProjectStacks(
  projects: ReadonlyArray<{ projectId: string; name: string }>,
  workQueueClient: Client<typeof WorkQueueService> | null | undefined,
  sessionToken: string,
): ProjectStacks[] {
  const [byProject, setByProject] = useState<ReadonlyMap<string, WorkQueueSummary[]>>(new Map());
  const projectIdsKey = projects.map((p) => p.projectId).join("\n");

  useEffect(() => {
    if (!workQueueClient) {
      setByProject(new Map());
      return;
    }
    let cancelled = false;
    const projectIds = projectIdsKey ? projectIdsKey.split("\n") : [];
    Promise.all(
      projectIds.map((projectId) =>
        workQueueClient
          .listProjectWorkQueues({ sessionToken, projectId, thisHostOnly: false })
          .then((res) => [projectId, res.stacks] as const)
          .catch(() => [projectId, [] as WorkQueueSummary[]] as const),
      ),
    ).then((entries) => {
      if (!cancelled) setByProject(new Map(entries));
    });
    return () => {
      cancelled = true;
    };
  }, [workQueueClient, sessionToken, projectIdsKey]);

  return projects
    .map((p) => ({ projectId: p.projectId, projectName: p.name, stacks: byProject.get(p.projectId) ?? [] }))
    .filter((p) => p.stacks.some((s) => s.address?.queueId));
}

/**
 * A stack's readable name: its title, else the label of the session that owns it (a queue's id is
 * its owning session's), else the raw queue id as the last resort.
 */
function stackName(summary: WorkQueueSummary, sessionsById: ReadonlyMap<string, SessionEntry>): string {
  if (summary.title.trim()) return summary.title;
  const queueId = summary.address?.queueId ?? "";
  const owner = sessionsById.get(queueId);
  return owner ? sessionDrawerLabel(owner) : queueId;
}

/**
 * The drawer's single **Stacks** box, directly above the Select bar. Clicking it opens a panel
 * upward (`sessions-drawer-stacks-panel`) listing stacks grouped per project
 * (`sessions-drawer-project-queues-<projectId>`); opening a stack hands its summary to the caller.
 * Renders nothing when no project has a stack.
 */
export function SessionDrawerStacksBox(props: {
  projectStacks: ProjectStacks[];
  sessions: SessionEntry[];
  onOpenStack: (summary: WorkQueueSummary) => void;
}): React.ReactElement | null {
  const { projectStacks, sessions, onOpenStack } = props;
  const [open, setOpen] = useState(false);
  if (projectStacks.length === 0) return null;
  const sessionsById = new Map(sessions.map((s) => [s.sessionId, s]));
  const total = projectStacks.reduce((n, p) => n + p.stacks.length, 0);
  const Chevron = open ? ChevronDown : ChevronUp;

  return (
    <div className="relative border-t border-border px-2 py-1">
      {open && (
        <div
          data-testid="sessions-drawer-stacks-panel"
          className="absolute inset-x-0 bottom-full z-20 max-h-[60vh] overflow-y-auto border-t border-border bg-background px-2 py-1 shadow-lg"
        >
          {projectStacks.map((p) => (
            <div key={p.projectId} data-testid={`sessions-drawer-project-queues-${p.projectId}`} className="py-1">
              <div className="px-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                {p.projectName}
              </div>
              {p.stacks.map((summary) => {
                const queueId = summary.address?.queueId ?? "";
                if (!queueId) return null;
                return (
                  <button
                    key={queueId}
                    type="button"
                    data-testid={`sessions-drawer-project-queue-${queueId}`}
                    onClick={() => {
                      setOpen(false);
                      onOpenStack(summary);
                    }}
                    className="flex w-full items-center gap-1 rounded px-1.5 py-1 text-left text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                  >
                    <span className="flex-1 truncate">{stackName(summary, sessionsById)}</span>
                    {summary.nodeCounts && (
                      <span className="flex-shrink-0 tabular-nums">{summary.nodeCounts.total}</span>
                    )}
                  </button>
                );
              })}
            </div>
          ))}
        </div>
      )}
      <button
        type="button"
        data-testid="sessions-drawer-stacks-box"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "flex w-full items-center gap-1 rounded border border-border px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground",
          open && "bg-muted text-foreground",
        )}
      >
        <span className="flex-1 text-left font-semibold uppercase tracking-wide">Stacks</span>
        <span className="tabular-nums">{total}</span>
        <Chevron className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
