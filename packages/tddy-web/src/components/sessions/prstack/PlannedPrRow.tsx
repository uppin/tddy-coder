import React from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { Button } from "../../ui/button";
import type { BranchResolution } from "../../../gen/connection_pb";
import { baseSyncView, canPullFromBase } from "./baseSyncStatus";
import { isNodeOrphaned } from "./isNodeOrphaned";
import { PlannedPrRowBadges } from "./PlannedPrRowBadges";
import { PlannedPrRowDetails } from "./PlannedPrRowDetails";
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
   * True while a mutation of this node's branch is in flight — a repoint or a pull from its base.
   *
   * One flag rather than one per operation, because it disables the same three controls either way:
   * repoint rebases and force-pushes the branch, and a pull merges or rebases the base into that very
   * branch. Any two of them running side by side is destructive rather than merely wasteful, and the
   * post-merge state where a node is both repointable *and* behind its new base is the normal one —
   * so "a repoint is running" and "a pull is running" are not a distinction a control can act on.
   * Each failure still reports itself separately (see {@link repointError} and {@link syncError}).
   */
  branchMutating?: boolean;
  /** False on the first row of the rendered order — its move-up control is inert. */
  canMoveUp?: boolean;
  /** False on the last row of the rendered order — its move-down control is inert. */
  canMoveDown?: boolean;
  /** Move this node one position in the persisted reading order. */
  onReorder?: (nodeId: string, direction: "up" | "down") => void;
  /** The daemon's reason for refusing or failing this node's last reorder, shown inline. */
  reorderError?: string;
  /** True while this node's reorder is in flight — both of its controls are disabled until it settles. */
  reordering?: boolean;
  /**
   * Pull this node's base into its branch. Rendered only when the branch is cleanly behind, so the
   * strategy is the operator's only remaining choice — the base comes from the comparison itself.
   */
  onSyncFromBase?: (nodeId: string, strategy: "merge" | "rebase") => void;
  /**
   * What the operator is told about this node's last pull from its base, shown inline. Either the
   * daemon's reason for refusing or failing it, or — for a pull that landed locally but whose push
   * did not (D32) — that the work is in the branch and not yet on the remote.
   */
  syncError?: string;
  /**
   * Every reason a spawn cannot succeed right now, each with the text to show. Non-empty *disables*
   * the Start-session button — it never replaces it, and never suppresses any of the row's own
   * information (D16): the row is the only place a planned PR's title, description, branch, base and
   * PR live, and a blocked operator needs all of it.
   */
  blockers?: StartBlocker[];
  /** The base branch the row states its child worktree would be created from. */
  baseBranchLabel?: string;
  /** True when this row's detail body is revealed. The row itself holds no state — see `PlannedPrList`. */
  expanded?: boolean;
  /** Reveal or hide this row's detail body. */
  onToggleExpanded?: (nodeId: string) => void;
  /** The titles of the planned PRs this node is stacked on — see `parentTitles`. */
  parentTitles?: string[];
  /**
   * The child session this row's status chip opens, resolved by `boundChildSession`. Empty when
   * neither the plan's recorded child nor the branch's current owner is a session the caller knows,
   * in which case the chip stays plain text rather than offering a control that selects nothing.
   */
  boundSessionId?: string;
  /** Select and attach {@link boundSessionId}, exactly as clicking that session in the drawer does. */
  onOpenSession?: (sessionId: string) => void;
}

/**
 * A single row in the planned-PR list, in three regions — one per block of the returned markup:
 *
 * - a **summary header** — the expand/collapse toggle carrying the title, then
 *   {@link PlannedPrRowBadges} and the CTA slot *beside* it rather than inside it (a button nested
 *   in a button is invalid markup, and would swallow the Start-session click into an expand);
 * - a **detail body** ({@link PlannedPrRowDetails}) holding everything else the node knows and the
 *   controls that act on its branch and its position;
 * - an **always-visible footer** for blockers and refusals. These stay outside the collapse boundary
 *   (D22): a reason the operator has to expand a row to discover is a fresh dead end — which is why
 *   it is the row itself, not the detail body, that renders them.
 *
 * The CTA slot holds either the live child's status chip — clickable when a bound session resolves —
 * or the Start-session button, disabled with a warning naming each blocker when a spawn cannot
 * succeed (D16, which reverses the earlier indicator that replaced the row's contents).
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
  branchMutating = false,
  canMoveUp = false,
  canMoveDown = false,
  onReorder,
  reorderError = "",
  reordering = false,
  onSyncFromBase,
  syncError = "",
  blockers = [],
  baseBranchLabel = "",
  expanded = false,
  onToggleExpanded,
  parentTitles = [],
  boundSessionId = "",
  onOpenSession,
}: PlannedPrRowProps) {
  // A node whose recorded child session has been deleted is workable again, so it shows the CTA
  // rather than a status chip for a session that no longer exists.
  const isSpawned = Boolean(node.sessionId) && !isNodeOrphaned(node, resolution);
  // A session is in progress when either source says so: `QueryBranch` resolves it server-side by
  // branch, while `inProgress` comes from the session list the caller already holds.
  const inProgressEffective =
    inProgress || Boolean(resolution?.session?.exists && resolution.session.isActive);
  // The blockers explain a Start-session button that cannot be pressed, so they are silent for a node
  // that has already been spawned: its child exists, and nothing about a base it will never be created
  // from is news.
  const isBlocked = !isSpawned && blockers.length > 0;
  // The same reasons as the warning, on the button itself, so hovering the disabled control answers why.
  const blockerSummary = blockers.map((b) => b.message).join("; ");
  // The chip becomes a control only when there is a session to select *and* a way to select it.
  const canOpenBoundSession = Boolean(boundSessionId) && Boolean(onOpenSession);
  // How the branch stands against the base the daemon actually compared it to. "Unknown" (an
  // unanswered poll, or a daemon that reports no comparison) renders nothing at all — a row says
  // nothing it does not know, and a failed comparison is never shown as clean (D27).
  const baseSync = baseSyncView(resolution);
  // The pull is offered only from a clean behind-count, and only for a branch there is something to
  // pull into: an unspawned node owns no branch to merge the base into. Holding the comparison
  // itself rather than a flag is what lets the controls name the count and the base they promise.
  const pullFromBase = canPullFromBase(baseSync) && node.branch ? baseSync : null;

  // The detail body's own id, so the header toggle can name what it reveals (`aria-controls`) rather
  // than leaving a screen reader to infer the relationship from document order.
  const detailsId = `pr-stack-row-details-${node.nodeId}`;

  const statusChip = (
    <span
      data-testid={`pr-stack-status-chip-${node.nodeId}`}
      className="flex-shrink-0 rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
    >
      {node.prStatus?.phase || node.childState || "spawned"}
    </span>
  );

  return (
    <div
      data-testid={`pr-stack-planned-pr-row-${node.nodeId}`}
      className="flex flex-col gap-1 rounded-md border border-border px-3 py-2"
    >
      <div className="flex items-center justify-between gap-3">
        <button
          type="button"
          data-testid={`pr-stack-row-toggle-${node.nodeId}`}
          aria-expanded={expanded}
          aria-controls={detailsId}
          onClick={() => onToggleExpanded?.(node.nodeId)}
          className="flex min-w-0 flex-1 items-center gap-1 text-left"
        >
          {expanded ? (
            <ChevronDown className="h-3.5 w-3.5 flex-shrink-0 text-muted-foreground" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5 flex-shrink-0 text-muted-foreground" />
          )}
          <span className="truncate text-sm font-medium">{node.title}</span>
        </button>
        {/* Everything the row states about its branch without the operator having to expand it —
            a sibling of the toggle above, never a child of it. */}
        <PlannedPrRowBadges
          node={node}
          inProgress={inProgressEffective}
          pr={resolution?.pr}
          baseSync={baseSync}
        />
        {canRepoint && (
          <Button
            data-testid={`pr-stack-repoint-${node.nodeId}`}
            size="sm"
            variant="outline"
            disabled={branchMutating}
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
          canOpenBoundSession ? (
            // The chip is wrapped rather than relabelled, so its own contract is untouched: it still
            // reads the child's phase and still answers to `pr-stack-status-chip-<nodeId>`. The
            // accessible name is stated rather than left to the chip's own text, which would
            // otherwise announce the phase alone ("spawned, button") with no hint that it navigates
            // — `title` does not override content-derived naming.
            <button
              type="button"
              data-testid={`pr-stack-session-${node.nodeId}`}
              aria-label={`Open child session ${boundSessionId} for ${node.title}`}
              title={`Open child session ${boundSessionId}`}
              onClick={() => onOpenSession?.(boundSessionId)}
              className="flex flex-shrink-0 items-center"
            >
              {statusChip}
            </button>
          ) : (
            statusChip
          )
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
      <PlannedPrRowDetails
        node={node}
        id={detailsId}
        expanded={expanded}
        baseBranchLabel={baseBranchLabel}
        worktree={resolution?.worktree}
        parentTitles={parentTitles}
        boundSessionId={boundSessionId}
        baseSync={baseSync}
        pullFromBase={pullFromBase}
        branchMutating={branchMutating}
        onSyncFromBase={onSyncFromBase}
        canMoveUp={canMoveUp}
        canMoveDown={canMoveDown}
        reordering={reordering}
        onReorder={onReorder}
      />
      {/* Each blocking issue in full, on its own line — the disabled button's reason, in the row
          rather than only in a tooltip. Outside the collapse boundary (D22). */}
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
      {/* A pull that did not happen leaves the row still reading "behind", and a pull that landed
          locally without reaching the remote leaves it reading "in sync" while the PR is not — both
          leave the row unable to state its own truth, so the reason lives outside the collapse
          boundary too: it must stay visible when the row is collapsed. */}
      {syncError && (
        <p
          data-testid={`pr-stack-sync-error-${node.nodeId}`}
          role="alert"
          className="text-xs text-destructive"
        >
          {syncError}
        </p>
      )}
      {/* A refused reorder moves nothing, which is indistinguishable from a click that was
          swallowed unless the row says why. */}
      {reorderError && (
        <p
          data-testid={`pr-stack-reorder-error-${node.nodeId}`}
          role="alert"
          className="text-xs text-destructive"
        >
          {reorderError}
        </p>
      )}
    </div>
  );
}
