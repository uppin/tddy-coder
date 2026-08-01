import React from "react";
import type { BranchResolution } from "../../../gen/connection_pb";
import type { BaseSyncView } from "./baseSyncStatus";
import type { StackNode } from "./stackPlan";

export interface PlannedPrRowBadgesProps {
  node: StackNode;
  /** True when a live session owns this node's branch. */
  inProgress: boolean;
  /** The branch's live GitHub PR, from the row's one branch resolution. */
  pr?: BranchResolution["pr"];
  /** How the branch stands against the base the daemon actually compared it to. */
  baseSync: BaseSyncView;
}

/** Tailwind classes for an internal-status badge, keyed by status kind. */
const INTERNAL_STATUS_BADGE_CLASSES: Record<string, string> = {
  "needs-repoint": "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-100",
  "has-conflicts": "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100",
  "ready-to-merge": "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100",
  blocked: "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100",
};

const BADGE_CLASS = "flex-shrink-0 rounded-full px-2 py-0.5 text-xs";
const MUTED_BADGE_CLASS = `${BADGE_CLASS} bg-muted text-muted-foreground`;

/**
 * A planned-PR row's status badges — everything the row states about its branch without the operator
 * having to expand it.
 *
 * A fragment rather than a wrapper element: these are siblings of the row's expand/collapse toggle
 * and of its CTA, sharing the header's flex row. Nesting them inside the toggle would put interactive
 * content in a button and swallow the CTA's own click.
 */
export function PlannedPrRowBadges({ node, inProgress, pr, baseSync }: PlannedPrRowBadgesProps) {
  const hasPr = Boolean(pr?.exists);
  // "Could not look up" is deliberately distinct from "this branch has no PR": conflating the two is
  // why a live open PR stayed invisible while the daemon held no GitHub credential.
  const prUnavailable = Boolean(pr?.unavailable);

  return (
    <>
      {inProgress && (
        <span
          data-testid={`pr-stack-in-progress-${node.nodeId}`}
          className={`${BADGE_CLASS} bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-100`}
        >
          in progress
        </span>
      )}
      {hasPr && (
        <a
          data-testid={`pr-stack-pr-link-${node.nodeId}`}
          href={pr!.url}
          target="_blank"
          rel="noreferrer"
          className="flex-shrink-0 text-xs text-blue-600 hover:underline dark:text-blue-400"
        >
          #{pr!.number.toString()}
        </a>
      )}
      {hasPr && (
        <span data-testid={`pr-stack-pr-state-${node.nodeId}`} className={MUTED_BADGE_CLASS}>
          {pr!.state}
        </span>
      )}
      {prUnavailable && (
        <span
          data-testid={`pr-stack-pr-unavailable-${node.nodeId}`}
          title={pr!.unavailableReason}
          className={MUTED_BADGE_CLASS}
        >
          PR status unavailable
        </span>
      )}
      {/* How the branch stands against its base, live on every poll — one badge, four mutually
          exclusive states, and silence only while the comparison is genuinely unknown. "In sync"
          is a badge rather than silence: without it a healthy row and a row whose poll has not
          answered would look identical. */}
      {baseSync.kind === "behind" && (
        <span
          data-testid={`pr-stack-base-behind-${node.nodeId}`}
          title={`${baseSync.behind} commit${baseSync.behind === 1 ? "" : "s"} on ${baseSync.baseBranch} not in this branch`}
          className={`${BADGE_CLASS} bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-100`}
        >
          {baseSync.behind} behind {baseSync.baseBranch}
        </span>
      )}
      {baseSync.kind === "in-sync" && (
        <span
          data-testid={`pr-stack-base-in-sync-${node.nodeId}`}
          className={`${BADGE_CLASS} bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100`}
        >
          in sync with {baseSync.baseBranch}
        </span>
      )}
      {baseSync.kind === "conflicts" && (
        <span
          data-testid={`pr-stack-base-conflicts-${node.nodeId}`}
          className={`${BADGE_CLASS} bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100`}
        >
          conflicts with {baseSync.baseBranch}
        </span>
      )}
      {/* Never rendered as clean: a failed comparison arrives byte-identical to a healthy one, so
          it carries its own discriminator and its own badge (D27). */}
      {baseSync.kind === "unavailable" && (
        <span
          data-testid={`pr-stack-base-sync-unavailable-${node.nodeId}`}
          title={baseSync.reason}
          className={MUTED_BADGE_CLASS}
        >
          base status unavailable
        </span>
      )}
      {node.internalStatus && (
        <span
          data-testid={`pr-stack-internal-status-badge-${node.nodeId}`}
          title={node.internalStatus.note ?? undefined}
          className={`${BADGE_CLASS} ${
            INTERNAL_STATUS_BADGE_CLASSES[node.internalStatus.kind] ??
            "bg-muted text-muted-foreground"
          }`}
        >
          {node.internalStatus.kind}
        </span>
      )}
    </>
  );
}
