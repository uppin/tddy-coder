import React, { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import {
  PrStackService,
  type ProjectStackSummary,
} from "tddy-client-web/gen/pr_stack_pb";

/**
 * The **Stacks** row in one project group's body (`#orchestrators` 9/11): the project's PR stacks on
 * every host, as the selected daemon's `ListProjectStacks` fans out to them. Opening a stack hands
 * its summary's address to the caller, which routes to the stack view. A project with no stacks —
 * or a daemon that cannot list them — renders no row.
 */
export function ProjectStacksRow(props: {
  projectId: string;
  prStackClient?: Client<typeof PrStackService> | null;
  sessionToken: string;
  onOpenStack: (summary: ProjectStackSummary) => void;
}): React.ReactElement | null {
  const { projectId, prStackClient, sessionToken, onOpenStack } = props;
  const [stacks, setStacks] = useState<ProjectStackSummary[]>([]);

  useEffect(() => {
    if (!prStackClient) return;
    let cancelled = false;
    prStackClient
      .listProjectStacks({ sessionToken, projectId, thisHostOnly: false })
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
  }, [prStackClient, sessionToken, projectId]);

  if (stacks.length === 0) return null;
  return (
    <div data-testid={`sessions-drawer-project-stacks-${projectId}`} className="px-2 py-1">
      <div className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
        Stacks
      </div>
      <div className="flex flex-col">
        {stacks.map((summary) => {
          const stackId = summary.address?.stackId ?? "";
          if (!stackId) return null;
          return (
            <button
              key={stackId}
              type="button"
              data-testid={`sessions-drawer-project-stack-${stackId}`}
              onClick={() => onOpenStack(summary)}
              className="rounded px-1.5 py-1 text-left text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            >
              {summary.title || stackId}
              {summary.nodeCounts ? ` (${summary.nodeCounts.total})` : ""}
            </button>
          );
        })}
      </div>
    </div>
  );
}
