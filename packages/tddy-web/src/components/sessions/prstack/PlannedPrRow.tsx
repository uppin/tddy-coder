import React from "react";
import { Button } from "../../ui/button";
import type { BranchResolution } from "../../../gen/connection_pb";
import { isNodeOrphaned } from "./isNodeOrphaned";
import type { StartBlocker } from "./startBlockers";
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
  /**
   * True when the node can be repointed: a predecessor has merged, or its base cannot be resolved
   * right now (any cause) — see `PlannedPrList`.
   */
  canRepoint?: boolean;
  /** Repoint this node onto {@link repointTarget} — drops the parents that do not own that branch. */
  onRepoint?: (nodeId: string) => void;
  /**
   * The branch a repoint would land this node on, named by the control so the operator knows where
   * the node goes before clicking. Empty when the project records no default branch, in which case
   * the control says "default branch" and the daemon resolves the real ref (D20).
   */
  repointTarget?: string;
  /** The daemon's reason for refusing or failing this node's last repoint, shown inline. */
  repointError?: string;
  /**
   * True while this node's repoint is in flight. Disables the control: a repoint of a node that owns a
   * branch rebases and force-pushes it, so a second concurrent run is destructive, not just wasteful.
   */
  repointing?: boolean;
  /**
   * Every reason a spawn cannot succeed right now, each with the text to show. Non-empty *disables*
   * the Start-session button — it never replaces it, and never suppresses any of the row's own
   * information (D16): the row is the only place a planned PR's title, description, branch, base and
   * PR live, and a blocked operator needs all of it.
   */
  blockers?: StartBlocker[];
  /** The base branch the row states its child worktree would be created from. */
  baseBranchLabel?: string;
}

/** Tailwind classes for an internal-status badge, keyed by status kind. */
const INTERNAL_STATUS_BADGE_CLASSES: Record<string, string> = {
  "needs-repoint": "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-100",
  "has-conflicts": "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100",
  "ready-to-merge": "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100",
  blocked: "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100",
};

/**
 * A single row in the planned-PR list, rendering the planned PR's full information whatever its
 * startability: title, description, the branch it owns or its planned branch, its base branch, its
 * worktree and its PR link/state. The CTA slot holds either the live child's status chip or the
 * Start-session button — disabled, with a warning naming each blocker, when a spawn cannot succeed
 * (D16, which reverses the earlier indicator that replaced the row's contents).
 */
export function PlannedPrRow({
  node,
  onStartSession,
  starting,
  inProgress = false,
  resolution,
  canRepoint = false,
  onRepoint,
  repointTarget = "",
  repointError = "",
  repointing = false,
  blockers = [],
  baseBranchLabel = "",
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
  // The blockers explain a Start-session button that cannot be pressed, so they are silent for a node
  // that has already been spawned: its child exists, and nothing about a base it will never be created
  // from is news.
  const isBlocked = !isSpawned && blockers.length > 0;
  // The same reasons as the warning, on the button itself, so hovering the disabled control answers why.
  const blockerSummary = blockers.map((b) => b.message).join("; ");

  return (
    <div
      data-testid={`pr-stack-planned-pr-row-${node.nodeId}`}
      className="flex flex-col gap-1 rounded-md border border-border px-3 py-2"
    >
      <div className="flex items-center justify-between gap-3">
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
          {/* The ref the child worktree would be created from. Rendered whatever the node's
              startability: "which branch is this waiting on" used to be legible only inside the
              blocked indicator's own text, so a healthy row never showed its base at all. */}
          <p
            data-testid={`pr-stack-base-branch-${node.nodeId}`}
            className="text-xs text-muted-foreground/70 truncate font-mono"
            title={`Base branch: ${baseBranchLabel}`}
          >
            base: {baseBranchLabel}
          </p>
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
            disabled={repointing}
            onClick={() => onRepoint?.(node.nodeId)}
          >
            {/* The target is named so the operator knows where the node lands before clicking, and it
                is the same value sent as `target_base_branch` (D18). An empty target means the project
                records no default branch — only the label degrades (D20). */}
            {repointTarget ? `Repoint to ${repointTarget}` : "Repoint to default branch"}
          </Button>
        )}
        {/* One CTA slot, two occupants: the live child's status chip, or the Start-session button —
            disabled when blocked rather than replaced, since it is the control a repoint re-enables. */}
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
            disabled={starting || isBlocked}
            title={blockerSummary || undefined}
            onClick={() => onStartSession(node)}
          >
            Start session
          </Button>
        )}
      </div>
      {/* Each blocking issue in full, on its own line — the disabled button's reason, in the row
          rather than only in a tooltip. */}
      {isBlocked && (
        <div
          data-testid={`pr-stack-start-warning-${node.nodeId}`}
          className="rounded-md bg-amber-100 px-2 py-0.5 text-xs text-amber-800 dark:bg-amber-900 dark:text-amber-100"
        >
          {blockers.map((blocker) => (
            <span key={blocker.kind} className="block">
              {blocker.message}
            </span>
          ))}
        </div>
      )}
      {/* A refusal the operator cannot see is a fresh dead end: the row stays blocked, so the reason
          it stayed blocked has to be visible. */}
      {repointError && (
        <p
          data-testid={`pr-stack-repoint-error-${node.nodeId}`}
          role="alert"
          className="text-xs text-destructive"
        >
          {repointError}
        </p>
      )}
    </div>
  );
}
