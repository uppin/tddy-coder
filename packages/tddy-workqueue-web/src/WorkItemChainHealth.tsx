/**
 * A work item's chain health chips (`#workqueue` 12/14): a leak chip naming how many commits leaked and
 * from which ancestor, and a linearity chip (linear / behind N / conflicting / unknown).
 *
 * Test ids: `work-item-leaks-${itemId}`, `work-item-linearity-${itemId}` (`data-state`).
 */
export interface WorkItemChainHealthProps {
  itemId: string;
  leaks: { sha: string; ancestor: string }[];
  line: { state: "linear" | "behind" | "conflicting" | "unknown"; by?: number };
}

export function WorkItemChainHealth(_props: WorkItemChainHealthProps) {
  // TODO(n12 leak-linearity): implement
  return null;
}
