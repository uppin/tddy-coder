import React from "react";
import type { BranchResolution, SessionEntry } from "../../../gen/connection_pb";
import { resolveNodeSession } from "../../../utils/resolveNodeSession";
import { PlannedPrRow } from "./PlannedPrRow";
import { resolveStackBase } from "./deriveStackBaseBranch";
import { resolveRepointTarget, startBlockers } from "./startBlockers";
import { topoSortStackNodes, type StackNode } from "./stackPlan";

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
}

/** Renders one row per planned `StackNode`, roots before their dependents. */
export function PlannedPrList({
  nodes,
  onStartSession,
  startingNodeId,
  sessions = [],
  branchResolutionByBranch = {},
  onRepoint,
  defaultBranch = "",
  repointErrorByNodeId = {},
}: PlannedPrListProps) {
  const ordered = topoSortStackNodes(nodes);
  const nodeById = new Map(nodes.map((n) => [n.nodeId, n]));

  return (
    <div data-testid="pr-stack-planned-pr-list" className="flex flex-col gap-2 overflow-y-auto p-3">
      {ordered.length === 0 ? (
        <p className="text-sm text-muted-foreground">No planned PRs yet.</p>
      ) : (
        ordered.map((node) => {
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
          const baseBranch =
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
              blockers={blockers}
              baseBranch={baseBranch}
            />
          );
        })
      )}
    </div>
  );
}
