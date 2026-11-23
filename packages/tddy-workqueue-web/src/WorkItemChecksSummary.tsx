import React from "react";

/**
 * A PR'ed work item's checks at a glance (`#workqueue` 6/14): running, failed and succeeded counts,
 * fed by the queue's status stream (never a browser poll).
 *
 * Test id: `work-item-checks-${itemId}`, with `work-item-checks-running|failed|succeeded-${itemId}`.
 */
export interface WorkItemChecksSummaryProps {
  itemId: string;
  running: number;
  failed: number;
  succeeded: number;
}

export function WorkItemChecksSummary({ itemId, running, failed, succeeded }: WorkItemChecksSummaryProps) {
  return (
    <p
      data-testid={`work-item-checks-${itemId}`}
      className="flex gap-3 text-xs text-muted-foreground"
      title="Checks at the PR's head: running, failed, succeeded"
    >
      <span>
        running <span data-testid={`work-item-checks-running-${itemId}`}>{running}</span>
      </span>
      <span>
        failed <span data-testid={`work-item-checks-failed-${itemId}`}>{failed}</span>
      </span>
      <span>
        succeeded <span data-testid={`work-item-checks-succeeded-${itemId}`}>{succeeded}</span>
      </span>
    </p>
  );
}
