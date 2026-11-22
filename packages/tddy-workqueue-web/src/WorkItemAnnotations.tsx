/**
 * A work item's labels (chips) and badges (by tone) (`#workqueue` 7/14) — the agent's and the user's
 * own annotations, shown apart from the derived internal-status badge.
 *
 * Test ids: `work-item-label-${itemId}-${label}`, `work-item-badge-${itemId}-${key}` (with `data-tone`).
 */
export interface WorkItemAnnotationsProps {
  itemId: string;
  labels: string[];
  badges: { key: string; text: string; tone: "info" | "success" | "warning" | "danger" }[];
}

export function WorkItemAnnotations(_props: WorkItemAnnotationsProps) {
  // TODO(n7 badges-labels): implement
  return null;
}
