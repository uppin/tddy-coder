import React from "react";
import type { BranchResolution, SessionEntry } from "../../../gen/connection_pb";
import { resolveNodeSession } from "../../../utils/resolveNodeSession";
import { PlannedPrRow } from "./PlannedPrRow";
import { branchlessNonMergedParent, resolveStackBase } from "./deriveStackBaseBranch";
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
  /** Repoint a node whose predecessor merged (drops the merged parent, rebases, re-targets the PR). */
  onRepoint?: (nodeId: string) => void;
}

/** Renders one row per planned `StackNode`, roots before their dependents. */
export function PlannedPrList({
  nodes,
  onStartSession,
  startingNodeId,
  sessions = [],
  branchResolutionByBranch = {},
  onRepoint,
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
          // Repoint is offered once at least one parent PR has merged (the node needs re-basing
          // onto the new effective base).
          const canRepoint = node.parents.some(
            (parentId) => nodeById.get(parentId)?.prStatus?.phase === "merged",
          );

          // Whether a spawn for this node has anything to be based onto. Three independent blockers:
          //  - a direct parent is non-merged and owns no branch, which is the daemon's own gate
          //    (`Stack::base_ref_for_spawn` refuses on *any* such parent, even beside a sibling that
          //    owns a good branch);
          //  - no ancestor at all owns a created branch, including via a merged parent whose own
          //    ancestors are blocked;
          //  - the base branch is absent from `origin`, which the child's worktree is fetched from —
          //    the failure otherwise lands inside `git fetch`, after the session dir was written.
          // A base whose resolution has not arrived is *unknown*, never missing: `useQueryBranch`
          // swallows failed polls, so blocking on it would be a permanent dead end of exactly the
          // kind this indicator exists to remove. A root (or all-merged) node needs no check at all —
          // its base is the project default branch, which exists by construction.
          //
          // None of it applies to a node that already owns a branch. Its spawn *resumes* that branch
          // (`work_on_selected_branch`), which creates no branch and resolves no chain base — the
          // daemon deliberately skips base resolution for it. Gating such a row would make an orphan
          // whose predecessor never pushed unrecoverable, even though the resume would have succeeded.
          const base = resolveStackBase(node, nodes);
          const blockingParent = branchlessNonMergedParent(node, nodes);
          const baseRemote =
            base.kind === "ancestor-branch"
              ? branchResolutionByBranch[base.branch]?.remote
              : undefined;
          const baseBranchMissing =
            !node.branch &&
            (blockingParent !== null ||
              base.kind === "no-ancestor-branch" ||
              baseRemote?.exists === false);
          // Name the branch the row is actually waiting for. A blocking parent takes precedence: it is
          // the unmet dependency, whereas `base` may name a sibling's branch that is already fine.
          const baseBranch =
            blockingParent !== null
              ? (blockingParent.branchSuggestion ?? "")
              : base.kind === "ancestor-branch"
                ? base.branch
                : base.kind === "no-ancestor-branch"
                  ? (base.plannedBranch ?? "")
                  : "";

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
              baseBranchMissing={baseBranchMissing}
              baseBranch={baseBranch}
            />
          );
        })
      )}
    </div>
  );
}
