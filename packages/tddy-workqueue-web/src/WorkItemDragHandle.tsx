import React from "react";
import { GripVertical } from "lucide-react";

/** Why a task bound to a PR stack cannot be moved — its place is its stack's. */
export const BOUND_TO_STACK_REASON =
  "Bound to a PR stack (it has a branch or PR, or sits on a line) — its position follows its stack and cannot be dragged";

/**
 * The grip at a task's left edge. Disabled, with the reason as its tooltip, for a task bound to a PR
 * stack; a free task starts a drag from it.
 */
export function WorkItemDragHandle({
  itemId,
  bound,
  dragging,
  onPointerDown,
}: {
  itemId: string;
  bound: boolean;
  dragging: boolean;
  onPointerDown?: (event: React.PointerEvent) => void;
}) {
  const label = bound ? BOUND_TO_STACK_REASON : "Drag to move this task";
  return (
    <span title={label} className="flex flex-shrink-0">
      <button
        type="button"
        data-testid={`work-item-drag-handle-${itemId}`}
        data-dragging={dragging || undefined}
        aria-label={label}
        title={label}
        disabled={bound}
        onPointerDown={bound ? undefined : onPointerDown}
        className={`flex-shrink-0 touch-none rounded p-0.5 text-muted-foreground ${
          bound ? "cursor-not-allowed opacity-40" : "cursor-grab hover:text-foreground active:cursor-grabbing"
        }`}
      >
        <GripVertical className="h-4 w-4" />
      </button>
    </span>
  );
}
