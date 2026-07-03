import React from "react";
import { Button } from "../../ui/button";
import type { StackNode } from "./stackPlan";

export interface PlannedPrRowProps {
  node: StackNode;
  onStartSession: (node: StackNode) => void;
  starting: boolean;
}

/** A single row in the planned-PR list: title/description plus a Start-session CTA or status chip. */
export function PlannedPrRow({ node, onStartSession, starting }: PlannedPrRowProps) {
  const isSpawned = Boolean(node.sessionId);

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
      </div>
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
