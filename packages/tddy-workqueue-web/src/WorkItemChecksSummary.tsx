/**
 * A PR'ed work item's checks at a glance (`#workqueue` 6/14): running, failed and succeeded counts,
 * fed by the queue's status stream (never a browser poll). Expanded rows reuse `CheckRow` from
 * `tddy-workspace-web`.
 *
 * Test id: `work-item-checks-${itemId}`, with `work-item-checks-running|failed|succeeded-${itemId}`.
 */
export interface WorkItemChecksSummaryProps {
  itemId: string;
  running: number;
  failed: number;
  succeeded: number;
}

export function WorkItemChecksSummary(_props: WorkItemChecksSummaryProps) {
  // TODO(n6 pr-and-session-link): implement
  return null;
}
