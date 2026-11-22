import React, { createContext, useContext } from "react";

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

/** The ancestors the leaks come from, nearest first, each once. */
function leakAncestors(leaks: WorkItemChainHealthProps["leaks"]): string[] {
  return leaks.reduce<string[]>((seen, leak) => (seen.includes(leak.ancestor) ? seen : [...seen, leak.ancestor]), []);
}

const CHIP = "flex-shrink-0 rounded-full px-2 py-0.5 text-xs";

const LINE_LABEL: Record<WorkItemChainHealthProps["line"]["state"], (by?: number) => string> = {
  linear: () => "linear",
  behind: (by) => `behind ${by ?? 0}`,
  conflicting: (by) => `conflicting, behind ${by ?? 0}`,
  unknown: () => "line unknown",
};

const LINE_CLASS: Record<WorkItemChainHealthProps["line"]["state"], string> = {
  linear: "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100",
  behind: "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-100",
  conflicting: "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100",
  unknown: "bg-muted text-muted-foreground",
};

export function WorkItemChainHealth({ itemId, leaks, line }: WorkItemChainHealthProps) {
  return (
    <>
      {leaks.length > 0 ? (
        <span
          data-testid={`work-item-leaks-${itemId}`}
          title={leaks.map((leak) => `${leak.sha.slice(0, 9)} from ${leak.ancestor}`).join("\n")}
          className={`${CHIP} bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-100`}
        >
          {leaks.length} leaked from {leakAncestors(leaks).join(", ")}
        </span>
      ) : null}
      <span
        data-testid={`work-item-linearity-${itemId}`}
        data-state={line.state}
        className={`${CHIP} ${LINE_CLASS[line.state]}`}
      >
        {LINE_LABEL[line.state](line.by)}
      </span>
    </>
  );
}

/** What the queue's status stream reports of an item's chain health. */
export type ItemChainHealth = Pick<WorkItemChainHealthProps, "leaks"> & {
  /** Absent for an item that owns no open PR: there is no line to read. */
  line?: WorkItemChainHealthProps["line"];
};

/** Chain health by item id; absent while unread. */
export const WorkItemChainHealthContext = createContext<ReadonlyMap<string, ItemChainHealth>>(new Map());

/** The chips of the row `itemId`, from the queue's status stream; nothing for an item without a reading. */
export function WorkItemChainHealthChips({ itemId }: { itemId: string }) {
  const health = useContext(WorkItemChainHealthContext).get(itemId);
  if (!health?.line) return null;
  return <WorkItemChainHealth itemId={itemId} leaks={health.leaks} line={health.line} />;
}
