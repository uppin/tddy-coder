import React from "react";
import { Button } from "../../ui/button";
import type { BranchResolution } from "../../../gen/connection_pb";
import type { BaseSyncView, BehindBaseSyncView } from "./baseSyncStatus";
import type { StackNode } from "./stackPlan";

export interface PlannedPrRowDetailsProps {
  node: StackNode;
  /** The element id the row header's toggle names in `aria-controls`. */
  id: string;
  /** True when the body is revealed. Hidden rather than unmounted while collapsed (D21). */
  expanded: boolean;
  /** The base branch the row states its child worktree would be created from. */
  baseBranchLabel: string;
  /** The node's branch on disk, from the row's one branch resolution. */
  worktree?: BranchResolution["worktree"];
  /** The titles of the planned PRs this node is stacked on — see `parentTitles`. */
  parentTitles: string[];
  /** The child session this row is bound to, resolved by `boundChildSession`. */
  boundSessionId: string;
  /** How the branch stands against its base — read here for the conflicting paths. */
  baseSync: BaseSyncView;
  /**
   * The comparison a pull would act on, or null when no pull is offered. Holding the view rather
   * than a flag is what lets the controls name the count and the base they promise.
   */
  pullFromBase: BehindBaseSyncView | null;
  /** True while a repoint or a pull of this node's branch is in flight. */
  branchMutating: boolean;
  onSyncFromBase?: (nodeId: string, strategy: "merge" | "rebase") => void;
  /** False on the first row of the rendered order — its move-up control is inert. */
  canMoveUp: boolean;
  /** False on the last row of the rendered order — its move-down control is inert. */
  canMoveDown: boolean;
  /** True while this node's reorder is in flight — both of its controls are disabled until it settles. */
  reordering: boolean;
  onReorder?: (nodeId: string, direction: "up" | "down") => void;
}

const DETAIL_LINE_CLASS = "text-xs text-muted-foreground truncate";

/**
 * A planned-PR row's detail body — everything the node knows beyond its summary header, plus the
 * controls that act on its branch and its position.
 *
 * Hidden rather than unmounted (D21): expansion, scroll position and the branch poll set all survive
 * a collapse, and none of the row's information is lost — it is one interaction away.
 */
export function PlannedPrRowDetails({
  node,
  id,
  expanded,
  baseBranchLabel,
  worktree,
  parentTitles,
  boundSessionId,
  baseSync,
  pullFromBase,
  branchMutating,
  onSyncFromBase,
  canMoveUp,
  canMoveDown,
  reordering,
  onReorder,
}: PlannedPrRowDetailsProps) {
  return (
    <div
      id={id}
      data-testid={`pr-stack-row-details-${node.nodeId}`}
      style={expanded ? undefined : { display: "none" }}
      className="flex min-w-0 flex-col"
    >
      {node.description && (
        <p className="text-xs text-muted-foreground truncate">{node.description}</p>
      )}
      {node.branch && (
        <p
          data-testid={`pr-stack-branch-${node.nodeId}`}
          className={`${DETAIL_LINE_CLASS} font-mono`}
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
      {worktree?.exists && (
        <p
          data-testid={`pr-stack-worktree-${node.nodeId}`}
          className={`${DETAIL_LINE_CLASS} font-mono`}
          title={worktree.path}
        >
          {worktree.path}
        </p>
      )}
      <p
        data-testid={`pr-stack-node-id-${node.nodeId}`}
        className={`${DETAIL_LINE_CLASS} font-mono`}
        title={`Node id: ${node.nodeId}`}
      >
        id: {node.nodeId}
      </p>
      {/* Parents are recorded as node ids, which say nothing to an operator reading the panel. */}
      {parentTitles.length > 0 && (
        <p
          data-testid={`pr-stack-parents-${node.nodeId}`}
          className={DETAIL_LINE_CLASS}
          title={`Stacked on: ${parentTitles.join(", ")}`}
        >
          stacked on: {parentTitles.join(", ")}
        </p>
      )}
      <p data-testid={`pr-stack-child-recipe-${node.nodeId}`} className={DETAIL_LINE_CLASS}>
        recipe: {node.childRecipe}
      </p>
      {node.childState && (
        <p data-testid={`pr-stack-child-state-${node.nodeId}`} className={DETAIL_LINE_CLASS}>
          state: {node.childState}
        </p>
      )}
      {boundSessionId && (
        <p
          data-testid={`pr-stack-child-session-${node.nodeId}`}
          className={`${DETAIL_LINE_CLASS} font-mono`}
          title={`Child session: ${boundSessionId}`}
        >
          session: {boundSessionId}
        </p>
      )}
      {/* The badge says there is a problem; this says where. The panel routes the operator to the
          agent to resolve them — it has `pr_resolve_conflicts` and an editing worktree. */}
      {baseSync.kind === "conflicts" && baseSync.paths.length > 0 && (
        <p
          data-testid={`pr-stack-base-conflict-paths-${node.nodeId}`}
          className={`${DETAIL_LINE_CLASS} font-mono`}
          title={`Conflicts in: ${baseSync.paths.join(", ")}`}
        >
          conflicts in: {baseSync.paths.join(", ")}
        </p>
      )}
      {/* Two plain controls rather than one with a menu: merge is the default because it disturbs
          no review anchors on the open PR and needs no force-push, and rebase is the explicit
          alternative beside it (D30). Both name the base the comparison produced, which is the
          base the pull sends. */}
      {pullFromBase && (
        <div className="mt-1 flex flex-wrap gap-2">
          <Button
            data-testid={`pr-stack-sync-merge-${node.nodeId}`}
            size="sm"
            disabled={branchMutating}
            onClick={() => onSyncFromBase?.(node.nodeId, "merge")}
          >
            Merge {pullFromBase.behind} commit{pullFromBase.behind === 1 ? "" : "s"} from{" "}
            {pullFromBase.baseBranch}
          </Button>
          <Button
            data-testid={`pr-stack-sync-rebase-${node.nodeId}`}
            size="sm"
            variant="outline"
            disabled={branchMutating}
            onClick={() => onSyncFromBase?.(node.nodeId, "rebase")}
          >
            Rebase onto {pullFromBase.baseBranch}
          </Button>
        </div>
      )}
      {/* The deliberate act that changes the reading order — nothing else moves a row. Inert at
          the ends of the rendered list rather than absent, so the control does not appear and
          disappear as rows move past it. */}
      <div className="mt-1 flex gap-2">
        <Button
          data-testid={`pr-stack-move-up-${node.nodeId}`}
          size="sm"
          variant="outline"
          disabled={!canMoveUp || reordering}
          onClick={() => onReorder?.(node.nodeId, "up")}
        >
          Move up
        </Button>
        <Button
          data-testid={`pr-stack-move-down-${node.nodeId}`}
          size="sm"
          variant="outline"
          disabled={!canMoveDown || reordering}
          onClick={() => onReorder?.(node.nodeId, "down")}
        >
          Move down
        </Button>
      </div>
    </div>
  );
}
