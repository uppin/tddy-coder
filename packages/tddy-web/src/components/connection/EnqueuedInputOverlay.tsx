import React from "react";
import type { OverlayModel } from "../../lib/terminalInputQueue";

/**
 * Single-line overlay of terminal input the server has not yet acknowledged.
 *
 * docs/ft/web/enqueued-input-overlay.md
 *
 * Rendered only while `visible` (the queue has been un-acked longer than the overlay delay).
 * Keyboard text shows inline; a run of mouse events shows as one glyph with its count; anything
 * past the single-line budget is summarised by a trailing overflow glyph and count.
 */

const MOUSE_GLYPH = "🖱";
const OVERFLOW_GLYPH = "⋯";

export interface EnqueuedInputOverlayProps {
  model: OverlayModel;
  visible: boolean;
}

export function EnqueuedInputOverlay({ model, visible }: EnqueuedInputOverlayProps) {
  if (!visible) return null;

  return (
    <div
      data-testid="enqueued-input-overlay"
      role="status"
      aria-label="Enqueued terminal input awaiting acknowledgement"
      style={{
        position: "absolute",
        left: 8,
        bottom: 8,
        maxWidth: "calc(100% - 16px)",
        display: "flex",
        alignItems: "center",
        gap: 4,
        whiteSpace: "nowrap",
        overflow: "hidden",
        padding: "2px 8px",
        borderRadius: 6,
        fontFamily: "var(--font-mono, monospace)",
        fontSize: 12,
        lineHeight: 1.4,
        color: "#fff",
        background: "rgba(0, 0, 0, 0.72)",
        pointerEvents: "none",
        zIndex: 5,
      }}
    >
      {model.segments.map((segment, i) =>
        segment.kind === "text" ? (
          <span key={i} data-testid="enqueued-input-text">
            {segment.text}
          </span>
        ) : (
          <span key={i} data-testid="enqueued-input-mouse" data-count={segment.count}>
            {MOUSE_GLYPH}×{segment.count}
          </span>
        ),
      )}
      {model.overflowCount > 0 && (
        <span data-testid="enqueued-input-overflow" data-count={model.overflowCount}>
          {OVERFLOW_GLYPH}+{model.overflowCount}
        </span>
      )}
    </div>
  );
}

export default EnqueuedInputOverlay;
