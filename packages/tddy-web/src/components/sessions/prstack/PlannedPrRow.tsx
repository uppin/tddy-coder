import React from "react";
import { Button } from "../../ui/button";
import type { StackNode } from "./stackPlan";

export interface PlannedPrRowProps {
  node: StackNode;
  onStartSession: (node: StackNode) => void;
  starting: boolean;
}

/** Tailwind classes for an internal-status badge, keyed by status kind. */
const INTERNAL_STATUS_BADGE_CLASSES: Record<string, string> = {
  "needs-repoint": "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-100",
  "has-conflicts": "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100",
  "ready-to-merge": "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100",
  blocked: "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-100",
};

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
