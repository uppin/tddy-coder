import React, { useState } from "react";
import type { BranchResolution, SessionEntry } from "../../../gen/connection_pb";
import { resolveNodeSession } from "../../../utils/resolveNodeSession";
import { PlannedPrRow } from "./PlannedPrRow";
import { boundChildSession } from "./boundChildSession";
import { resolveStackBase } from "./deriveStackBaseBranch";
import { orderStackNodes } from "./orderStackNodes";
import { parentTitles } from "./parentTitles";
import { resolveRepointTarget, startBlockers } from "./startBlockers";
import type { StackNode } from "./stackPlan";

export interface PlannedPrListProps {
  nodes: StackNode[];
  onStartSession: (node: StackNode) => void;
  startingNodeId: string | null;
  /** All sessions — used to resolve each node's in-progress child session by branch. */
  sessions?: SessionEntry[];
  /**
   * One-call branch resolution (worktree + session + remote + PR) keyed by branch, from
   * `useQueryBranch` — the screen's only source of live branch and PR state. Covers each node's own
   * branch *and* each node's base branch, whose `remote` leg decides whether the node can be started
   * at all.
   */
  branchResolutionByBranch?: Record<string, BranchResolution>;
  /** Repoint a node onto the target its control names (drops the parents that do not own it). */
  onRepoint?: (nodeId: string) => void;
  /**
   * The project's default branch (`ProjectEntry.main_branch_ref`) — the base of a root node and the
   * repoint target when no parent can serve as a base. Empty for a legacy project that stores none,
   * which degrades the label only: the daemon resolves the real ref at click time (D20).
   */
  defaultBranch?: string;
  /** The daemon's reason per node whose last repoint was refused or failed, keyed by node id. */
  repointErrorByNodeId?: Record<string, string>;
  /**
   * Nodes with a mutation of their branch in flight — a repoint or a pull. Disables that row's
   * repoint control *and* both of its pull controls, since all three rewrite the same branch's refs.
   */
  branchMutatingNodeIds?: ReadonlySet<string>;
  /** Move a node one position in the persisted reading order. */
  onReorder?: (nodeId: string, direction: "up" | "down") => void;
  /** The daemon's reason per node whose last reorder was refused or failed, keyed by node id. */
  reorderErrorByNodeId?: Record<string, string>;
  /** Nodes whose reorder is in flight — both of their controls are disabled until it settles. */
  reorderingNodeIds?: ReadonlySet<string>;
  /** Pull a node's base into its branch, by the strategy the clicked control named. */
  onSyncFromBase?: (nodeId: string, strategy: "merge" | "rebase") => void;
  /** The daemon's reason per node whose last pull was refused or failed, keyed by node id. */
  syncErrorByNodeId?: Record<string, string>;
  /**
   * Select and attach a node's bound child session — the same act as clicking that session in the
   * drawer. Absent when the caller offers no navigation, in which case a row's status chip stays
   * plain text rather than becoming a control that goes nowhere.
   */
  onOpenSession?: (sessionId: string) => void;
}

/**
 * Renders one row per planned `StackNode`, in the order the plan persists (see `orderStackNodes`).
 *
 * The list owns which rows are expanded, rather than each row holding its own state, so the set is a
 * property of the list that survives every re-render a poll tick causes.
 */
export function PlannedPrList({
  nodes,
  onStartSession,
  startingNodeId,
  sessions = [],
  branchResolutionByBranch = {},
  onRepoint,
  defaultBranch = "",
  repointErrorByNodeId = {},
  branchMutatingNodeIds = new Set<string>(),
  onReorder,
  reorderErrorByNodeId = {},
  reorderingNodeIds = new Set<string>(),
  onSyncFromBase,
  syncErrorByNodeId = {},
  onOpenSession,
}: PlannedPrListProps) {
  const ordered = orderStackNodes(nodes);
  const nodeById = new Map(nodes.map((n) => [n.nodeId, n]));
  const [expandedNodeIds, setExpandedNodeIds] = useState<ReadonlySet<string>>(new Set());

  const toggleExpanded = (nodeId: string) => {
    setExpandedNodeIds((prev) => {
      const next = new Set(prev);
      if (next.has(nodeId)) {
        next.delete(nodeId);
      } else {
        next.add(nodeId);
      }
      return next;
    });
  };

  return (
    <div data-testid="pr-stack-planned-pr-list" className="flex flex-col gap-2 overflow-y-auto p-3">
      {ordered.length === 0 ? (
        <p className="text-sm text-muted-foreground">No planned PRs yet.</p>
      ) : (
        ordered.map((node, index) => {
          // In progress when a live session owns the node's branch.
          const owner = resolveNodeSession(node, sessions);
          const inProgress = Boolean(owner?.isActive);
          const resolution = node.branch ? branchResolutionByBranch[node.branch] : undefined;
          // Every reason this node cannot be started right now — see `startBlockers` for the rules.
          const blockers = startBlockers(node, nodes, branchResolutionByBranch);
          // Repoint is offered for the original merged-predecessor case *and* whenever the base
          // cannot be resolved right now, for any cause (D17). The plan's own `pr_status` is written
          // by the orchestrator agent and is stale in exactly the dead-end case — a merged
          // predecessor whose branch was deleted — so gating on it alone left that node unrecoverable.
          const canRepoint =
            blockers.length > 0 ||
            node.parents.some((parentId) => nodeById.get(parentId)?.prStatus?.phase === "merged");

          // The base branch as the row states it. `resolveStackBase` distinguishes the three cases a
          // plain name cannot: a concrete ancestor ref, the project default (a root or an all-merged
          // chain), and a chain whose ancestors own no ref at all — which is named in words rather
          // than with the blocked ancestor's *planned* branch, since that branch does not exist and
          // would read as a base the child could be created from.
          const base = resolveStackBase(node, nodes);
          const baseBranchLabel =
            base.kind === "ancestor-branch"
              ? base.branch
              : base.kind === "default-branch"
                ? // Empty for a legacy project with no stored `main_branch_ref` (D20).
                  defaultBranch || "default branch"
                : "no predecessor branch yet";

          return (
            <PlannedPrRow
              key={node.nodeId}
              node={node}
              onStartSession={onStartSession}
              starting={startingNodeId === node.nodeId}
              inProgress={inProgress}
              resolution={resolution}
              canRepoint={canRepoint}
              onRepoint={onRepoint}
              repointTarget={resolveRepointTarget(
                node,
                nodes,
                branchResolutionByBranch,
                defaultBranch,
              )}
              repointError={repointErrorByNodeId[node.nodeId]}
              branchMutating={branchMutatingNodeIds.has(node.nodeId)}
              // The ends of the *rendered* order, which is the order the operator is looking at —
              // the persisted positions themselves may be sparse or start anywhere.
              canMoveUp={index > 0}
              canMoveDown={index < ordered.length - 1}
              onReorder={onReorder}
              reorderError={reorderErrorByNodeId[node.nodeId]}
              reordering={reorderingNodeIds.has(node.nodeId)}
              onSyncFromBase={onSyncFromBase}
              syncError={syncErrorByNodeId[node.nodeId]}
              blockers={blockers}
              baseBranchLabel={baseBranchLabel}
              expanded={expandedNodeIds.has(node.nodeId)}
              onToggleExpanded={toggleExpanded}
              parentTitles={parentTitles(node, nodes)}
              boundSessionId={boundChildSession(node, resolution, sessions)}
              onOpenSession={onOpenSession}
            />
          );
        })
      )}
    </div>
  );
}
