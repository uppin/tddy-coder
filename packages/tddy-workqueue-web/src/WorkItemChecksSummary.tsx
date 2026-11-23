import React from "react";

/**
 * A PR'ed work item's checks at a glance (`#workqueue` 6/14), small enough for the task box's header:
 * plain coloured numbers — failed in red, succeeded in green; a zero count is not drawn. Checks still
 * running are not counted here: they are the spinner turning around the PR number (`WorkItemPrBadge`). Fed by the queue's status
 * stream (never a browser poll).
 *
 * Test id: `work-item-checks-${itemId}`, with `work-item-checks-failed|succeeded-${itemId}`.
 */
export interface WorkItemChecksSummaryProps {
  itemId: string;
  running: number;
  failed: number;
  succeeded: number;
}

export function WorkItemChecksSummary({ itemId, running, failed, succeeded }: WorkItemChecksSummaryProps) {
  return (
    <span
      data-testid={`work-item-checks-${itemId}`}
      className="flex flex-shrink-0 items-center gap-1.5 text-xs font-semibold"
      title={`Checks at the PR's head: ${failed} failed, ${succeeded} succeeded${running > 0 ? `, ${running} running` : ""}`}
    >
      {failed > 0 && (
        <span data-testid={`work-item-checks-failed-${itemId}`} className="text-red-600" title={`${failed} failed`}>
          {failed}
        </span>
      )}
      {succeeded > 0 && (
        <span data-testid={`work-item-checks-succeeded-${itemId}`} className="text-green-600" title={`${succeeded} succeeded`}>
          {succeeded}
        </span>
      )}
    </span>
  );
}
