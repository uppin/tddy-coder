import React, { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import {
  WorkQueueService,
  type WorkQueueSummary,
} from "tddy-client-web/gen/work_queue_pb";

/**
 * The **Stacks** row in one project group's body (`#orchestrators` 9/11): the project's PR stacks on
 * every host, as the selected daemon's `ListProjectWorkQueues` fans out to them. Opening a stack hands
 * its summary's address to the caller, which routes to the stack view. A project with no stacks —
 * or a daemon that cannot list them — renders no row.
 */
export function ProjectWorkQueuesRow(props: {
  projectId: string;
  workQueueClient?: Client<typeof WorkQueueService> | null;
  sessionToken: string;
  onOpenStack: (summary: WorkQueueSummary) => void;
}): React.ReactElement | null {
  const { projectId, workQueueClient, sessionToken, onOpenStack } = props;
  const [stacks, setStacks] = useState<WorkQueueSummary[]>([]);

  useEffect(() => {
    if (!workQueueClient) return;
    let cancelled = false;
    workQueueClient
      .listProjectWorkQueues({ sessionToken, projectId, thisHostOnly: false })
      .then((res) => {
        if (!cancelled) setStacks(res.stacks);
      })
      .catch(() => {
        // No row is the honest rendering of a listing that did not answer.
        if (!cancelled) setStacks([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workQueueClient, sessionToken, projectId]);

  if (stacks.length === 0) return null;
  return (
    <div data-testid={`sessions-drawer-project-queues-${projectId}`} className="px-2 py-1">
      <div className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
        Stacks
      </div>
      <div className="flex flex-col">
        {stacks.map((summary) => {
          const queueId = summary.address?.queueId ?? "";
          if (!queueId) return null;
          return (
            <button
              key={queueId}
              type="button"
              data-testid={`sessions-drawer-project-queue-${queueId}`}
              onClick={() => onOpenStack(summary)}
              className="rounded px-1.5 py-1 text-left text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            >
              {summary.title || queueId}
              {summary.nodeCounts ? ` (${summary.nodeCounts.total})` : ""}
            </button>
          );
        })}
      </div>
    </div>
  );
}
