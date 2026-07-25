import React from "react";
import type { PrStatusView, SessionEntry } from "../../../gen/connection_pb";
import { resolveNodeSession } from "../../../utils/resolveNodeSession";
import { PlannedPrRow } from "./PlannedPrRow";
import { topoSortStackNodes, type StackNode } from "./stackPlan";

export interface PlannedPrListProps {
  nodes: StackNode[];
  onStartSession: (node: StackNode) => void;
  startingNodeId: string | null;
  /** All sessions — used to resolve each node's in-progress child session by branch. */
  sessions?: SessionEntry[];
  /** Live GitHub PR status keyed by branch (from `usePrStatus`). */
  prStatusByBranch?: Record<string, PrStatusView>;
  /** Repoint a node whose predecessor merged (drops the merged parent, rebases, re-targets the PR). */
  onRepoint?: (nodeId: string) => void;
}

/** Renders one row per planned `StackNode`, roots before their dependents. */
export function PlannedPrList({
  nodes,
  onStartSession,
  startingNodeId,
  sessions = [],
  prStatusByBranch = {},
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
          const prStatus = node.branch ? prStatusByBranch[node.branch] : undefined;
          // Repoint is offered once at least one parent PR has merged (the node needs re-basing
          // onto the new effective base).
          const canRepoint = node.parents.some(
            (parentId) => nodeById.get(parentId)?.prStatus?.phase === "merged",
          );

          return (
            <PlannedPrRow
              key={node.nodeId}
              node={node}
              onStartSession={onStartSession}
              starting={startingNodeId === node.nodeId}
              inProgress={inProgress}
              prStatus={prStatus}
              canRepoint={canRepoint}
              onRepoint={onRepoint}
            />
          );
        })
      )}
    </div>
  );
}
