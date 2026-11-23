import React, { createContext, useContext } from "react";
import { ArrowDown, Check, CircleHelp, Droplet, TriangleAlert } from "lucide-react";

/**
 * A work item's chain health (`#workqueue` 12/14) as icons on its line: a leak icon naming how many
 * commits leaked and from which ancestor, and a linearity icon (linear / behind N / conflicting / unknown).
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

const ICON_CLASS = "flex h-5 w-5 items-center justify-center rounded-full border border-background shadow-sm";

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

const LINE_ICON = {
  linear: Check,
  behind: ArrowDown,
  conflicting: TriangleAlert,
  unknown: CircleHelp,
} as const;

/**
 * A line's health as icons, drawn **on the line** joining a task to its base (`WorkItemConnectors`) —
 * lineage belongs to the link between two PRs, not to a box: a leak icon naming the ancestors commits
 * leaked from, and a linearity icon (linear / behind / conflicting / unknown). The detail is the
 * icon's tooltip and accessible name.
 */
export function WorkItemChainHealth({ itemId, leaks, line }: WorkItemChainHealthProps) {
  const Icon = LINE_ICON[line.state];
  const label = `Line: ${LINE_LABEL[line.state](line.by)}`;
  const leakLabel = `${leaks.length} leaked from ${leakAncestors(leaks).join(", ")}\n${leaks
    .map((leak) => `${leak.sha.slice(0, 9)} from ${leak.ancestor}`)
    .join("\n")}`;
  return (
    <>
      {leaks.length > 0 ? (
        <span
          data-testid={`work-item-leaks-${itemId}`}
          title={leakLabel}
          aria-label={leakLabel}
          className={`${ICON_CLASS} bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-100`}
        >
          <Droplet className="h-3 w-3" />
        </span>
      ) : null}
      <span
        data-testid={`work-item-linearity-${itemId}`}
        data-state={line.state}
        title={label}
        aria-label={label}
        className={`${ICON_CLASS} ${LINE_CLASS[line.state]}`}
      >
        <Icon className="h-3 w-3" />
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

/** The line icons of the line into `itemId`, from the queue's status stream; nothing without a reading. */
export function WorkItemChainHealthChips({ itemId }: { itemId: string }) {
  const health = useContext(WorkItemChainHealthContext).get(itemId);
  if (!health?.line) return null;
  return <WorkItemChainHealth itemId={itemId} leaks={health.leaks} line={health.line} />;
}

/**
 * The count of commits a task's branch is behind its base, as a circular badge on the box's top-right
 * corner (`work-item-behind-badge-${itemId}`); nothing when it is not behind. Red when it also conflicts.
 */
export function WorkItemBehindBadge({ itemId }: { itemId: string }) {
  const line = useContext(WorkItemChainHealthContext).get(itemId)?.line;
  if (!line || (line.state !== "behind" && line.state !== "conflicting") || !line.by) return null;
  return (
    <span
      data-testid={`work-item-behind-badge-${itemId}`}
      data-state={line.state}
      title={LINE_LABEL[line.state](line.by)}
      className={`absolute -right-2 -top-2 flex h-5 min-w-5 items-center justify-center rounded-full px-1 text-[10px] font-semibold leading-none text-white ${
        line.state === "conflicting" ? "bg-red-600" : "bg-amber-600"
      }`}
    >
      {line.by}
    </span>
  );
}
