import React from "react";
import type { WorkItemBadge } from "./workQueue";

/**
 * A work item's labels (chips) and badges (by tone) (`#workqueue` 7/14) — the agent's and the user's
 * own annotations, shown apart from the derived internal-status badge.
 *
 * Test ids: `work-item-label-${itemId}-${label}`, `work-item-badge-${itemId}-${key}` (with `data-tone`).
 */
export interface WorkItemAnnotationsProps {
  itemId: string;
  labels: string[];
  badges: WorkItemBadge[];
}

const CHIP_CLASS = "flex-shrink-0 rounded-full px-2 py-0.5 text-xs";

const TONE_CLASSES: Record<WorkItemBadge["tone"], string> = {
  info: "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-100",
  success: "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100",
  warning: "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-100",
  danger: "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100",
};

export function WorkItemAnnotations({ itemId, labels, badges }: WorkItemAnnotationsProps) {
  return (
    <>
      {labels.map((label) => (
        <span
          key={`label-${label}`}
          data-testid={`work-item-label-${itemId}-${label}`}
          className={`${CHIP_CLASS} border border-border text-muted-foreground`}
        >
          {label}
        </span>
      ))}
      {badges.map((badge) => (
        <span
          key={`badge-${badge.key}`}
          data-testid={`work-item-badge-${itemId}-${badge.key}`}
          data-tone={badge.tone}
          title={badge.key}
          className={`${CHIP_CLASS} ${TONE_CLASSES[badge.tone]}`}
        >
          {badge.text}
        </span>
      ))}
    </>
  );
}
