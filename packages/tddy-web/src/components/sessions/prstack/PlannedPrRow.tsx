import React from "react";
import { Button } from "../../ui/button";
import type { BranchResolution, PrStatusView } from "../../../gen/connection_pb";
import type { StackNode } from "./stackPlan";

export interface PlannedPrRowProps {
  node: StackNode;
  onStartSession: (node: StackNode) => void;
  starting: boolean;
  /** True when a live session owns this node's branch (shows the in-progress indicator). */
  inProgress?: boolean;
  /** Live GitHub PR status for this node's branch (number/link/state), when a PR exists. */
  prStatus?: PrStatusView;
  /**
   * One-call branch resolution (worktree + in-progress session + PR) for this node's branch, from
   * `QueryBranch`. When present it drives the worktree indicator, in-progress badge, and PR
   * link/state; the legacy `inProgress`/`prStatus` props remain the fallback source.
   */
  resolution?: BranchResolution;
  /** True when a predecessor has merged and the node can be repointed. */
  canRepoint?: boolean;
  /** Repoint this node — drops merged parents, rebases, and re-targets the open PR base. */
  onRepoint?: (nodeId: string) => void;
}

/** Tailwind classes for an internal-status badge, keyed by status kind. */
const INTERNAL_STATUS_BADGE_CLASSES: Record<string, string> = {
  "needs-repoint": "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-100",
  "has-conflicts": "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100",
  "ready-to-merge": "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100",
  blocked: "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100",
};

/** A single row in the planned-PR list: title/description plus a Start-session CTA or status chip. */
export function PlannedPrRow({
  node,
  onStartSession,
  starting,
  inProgress = false,
  prStatus,
  resolution,
  canRepoint = false,
  onRepoint,
}: PlannedPrRowProps) {
  const isSpawned = Boolean(node.sessionId);
  // Prefer the one-call QueryBranch resolution when present; otherwise fall back to the legacy
  // per-surface props so existing callers keep working.
  const worktree = resolution?.worktree;
  const showWorktree = Boolean(worktree?.exists);
  const inProgressEffective =
    inProgress || Boolean(resolution?.session?.exists && resolution.session.isActive);
  const pr = resolution?.pr ?? prStatus;
  const hasPr = Boolean(pr?.exists);

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
      {isSpawned ? (
        <span
          data-testid={`pr-stack-status-chip-${node.nodeId}`}
          className="flex-shrink-0 rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
        >
          {node.prStatus?.phase || node.childState || "spawned"}
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
