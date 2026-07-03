import React from "react";
import { PlannedPrRow } from "./PlannedPrRow";
import { topoSortStackNodes, type StackNode } from "./stackPlan";

export interface PlannedPrListProps {
  nodes: StackNode[];
  onStartSession: (node: StackNode) => void;
  startingNodeId: string | null;
}

/** Renders one row per planned `StackNode`, roots before their dependents. */
export function PlannedPrList({ nodes, onStartSession, startingNodeId }: PlannedPrListProps) {
  const ordered = topoSortStackNodes(nodes);

  return (
    <div data-testid="pr-stack-planned-pr-list" className="flex flex-col gap-2 overflow-y-auto p-3">
      {ordered.length === 0 ? (
        <p className="text-sm text-muted-foreground">No planned PRs yet.</p>
      ) : (
        ordered.map((node) => (
          <PlannedPrRow
            key={node.nodeId}
            node={node}
            onStartSession={onStartSession}
            starting={startingNodeId === node.nodeId}
          />
        ))
      )}
    </div>
  );
}
