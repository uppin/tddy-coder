import React from "react";
import { Button } from "../../ui/button";
import type { BranchResolution } from "../../../gen/connection_pb";
import { isNodeOrphaned } from "./isNodeOrphaned";
import type { StackNode } from "./stackPlan";

export interface PlannedPrRowProps {
  node: StackNode;
  onStartSession: (node: StackNode) => void;
  starting: boolean;
  /** True when a live session owns this node's branch (shows the in-progress indicator). */
  inProgress?: boolean;
  /**
   * One-call branch resolution (worktree + in-progress session + PR) for this node's branch, from
   * `QueryBranch`. It is the only source of the row's PR link/state — the row makes no second PR
   * lookup — and it also drives the worktree indicator, the in-progress badge, and whether the
   * node's recorded child session still exists.
   */
  resolution?: BranchResolution;
  /** True when a predecessor has merged and the node can be repointed. */
  canRepoint?: boolean;
  /** Repoint this node — drops merged parents, rebases, and re-targets the open PR base. */
  onRepoint?: (nodeId: string) => void;
  /**
   * True when a spawn has nothing to be based onto yet: no ancestor owns a created branch, or the
   * base branch is absent from `origin`. Blocks the Start-session CTA rather than disabling it, since
   * the spawn would otherwise fail inside `git fetch` after a session directory was already written.
   */
  baseBranchMissing?: boolean;
  /** The base branch a blocked node is waiting for, named in the blocked indicator. */
  baseBranch?: string;
}

/** Tailwind classes for an internal-status badge, keyed by status kind. */
const INTERNAL_STATUS_BADGE_CLASSES: Record<string, string> = {
  "needs-repoint": "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-100",
  "has-conflicts": "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100",
  "ready-to-merge": "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100",
  blocked: "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100",
};

/**
 * A single row in the planned-PR list: title, description and branch, plus exactly one of a
 * Start-session CTA, a status chip, or a blocked "Missing branch" indicator.
 */
export function PlannedPrRow({
  node,
  onStartSession,
  starting,
  inProgress = false,
  resolution,
  canRepoint = false,
  onRepoint,
  baseBranchMissing = false,
  baseBranch = "",
}: PlannedPrRowProps) {
  // A node whose recorded child session has been deleted is workable again, so it shows the CTA
  // rather than a status chip for a session that no longer exists.
  const isSpawned = Boolean(node.sessionId) && !isNodeOrphaned(node, resolution);
  const worktree = resolution?.worktree;
  const showWorktree = Boolean(worktree?.exists);
  // A session is in progress when either source says so: `QueryBranch` resolves it server-side by
  // branch, while `inProgress` comes from the session list the caller already holds.
  const inProgressEffective =
    inProgress || Boolean(resolution?.session?.exists && resolution.session.isActive);
  const pr = resolution?.pr;
  const hasPr = Boolean(pr?.exists);
  // "Could not look up" is deliberately distinct from "this branch has no PR": conflating the two is
  // why a live open PR stayed invisible while the daemon held no GitHub credential.
  const prUnavailable = Boolean(pr?.unavailable);

  return (
    <div
      data-testid={`pr-stack-planned-pr-row-${node.nodeId}`}
      className="flex items-center justify-between gap-3 rounded-md border border-border px-3 py-2"
    >
      <div className="min-w-0">
        <p className="text-sm font-medium truncate">{node.title}</p>
        {node.description && (
          <p className="text-xs text-muted-foreground truncate">{node.description}</p>
        )}
        {node.branch && (
          <p
            data-testid={`pr-stack-branch-${node.nodeId}`}
            className="text-xs text-muted-foreground truncate font-mono"
            title={node.branch}
          >
            {node.branch}
          </p>
        )}
        {/* A suggestion names no ref — the branch does not exist yet — so it is marked as planned
            rather than shown as the branch the node owns. */}
        {!node.branch && node.branchSuggestion && (
          <p
            data-testid={`pr-stack-planned-branch-${node.nodeId}`}
            className="text-xs text-muted-foreground/70 truncate font-mono italic"
            title={`Planned branch, not created yet: ${node.branchSuggestion}`}
          >
            planned: {node.branchSuggestion}
          </p>
        )}
        {showWorktree && (
          <p
            data-testid={`pr-stack-worktree-${node.nodeId}`}
            className="text-xs text-muted-foreground truncate font-mono"
            title={worktree!.path}
          >
            {worktree!.path}
          </p>
        )}
      </div>
      {inProgressEffective && (
        <span
          data-testid={`pr-stack-in-progress-${node.nodeId}`}
          className="flex-shrink-0 rounded-full bg-blue-100 px-2 py-0.5 text-xs text-blue-800 dark:bg-blue-900 dark:text-blue-100"
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
        <span
          data-testid={`pr-stack-pr-state-${node.nodeId}`}
          className="flex-shrink-0 rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
        >
          {pr!.state}
        </span>
      )}
      {prUnavailable && (
        <span
          data-testid={`pr-stack-pr-unavailable-${node.nodeId}`}
          title={pr!.unavailableReason}
          className="flex-shrink-0 rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
        >
          PR status unavailable
        </span>
      )}
      {node.internalStatus && (
        <span
          data-testid={`pr-stack-internal-status-badge-${node.nodeId}`}
          title={node.internalStatus.note ?? undefined}
          className={`flex-shrink-0 rounded-full px-2 py-0.5 text-xs ${
            INTERNAL_STATUS_BADGE_CLASSES[node.internalStatus.kind] ??
            "bg-muted text-muted-foreground"
          }`}
        >
          {node.internalStatus.kind}
        </span>
      )}
      {canRepoint && (
        <Button
          data-testid={`pr-stack-repoint-${node.nodeId}`}
          size="sm"
          variant="outline"
          onClick={() => onRepoint?.(node.nodeId)}
        >
          Repoint
        </Button>
      )}
      {/* One CTA slot, three mutually exclusive occupants: the live child's status chip, the blocked
          indicator for a node with no base to branch from, or the Start-session button. */}
      {isSpawned ? (
        <span
          data-testid={`pr-stack-status-chip-${node.nodeId}`}
          className="flex-shrink-0 rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
        >
          {node.prStatus?.phase || node.childState || "spawned"}
        </span>
      ) : baseBranchMissing ? (
        <span
          data-testid={`pr-stack-missing-branch-${node.nodeId}`}
          title={
            baseBranch
              ? `Waiting for ${baseBranch} on origin — start the predecessor first`
              : "Waiting for a predecessor to create its branch"
          }
          className="flex-shrink-0 rounded-full bg-amber-100 px-2 py-0.5 text-xs text-amber-800 dark:bg-amber-900 dark:text-amber-100"
        >
          {baseBranch ? `Missing branch: ${baseBranch}` : "Missing branch"}
        </span>
      ) : (
        <Button
          data-testid={`pr-stack-start-session-${node.nodeId}`}
          size="sm"
          disabled={starting}
          onClick={() => onStartSession(node)}
        >
          Start session
        </Button>
      )}
    </div>
  );
}
