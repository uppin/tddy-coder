import React from "react";

export interface SessionRuntimeStatusBarProps {
  /** Verbatim line or formatted snapshot from `TddyRemote` stream; empty before first event. */
  statusLine: string;
}

/**
 * Fixed strip showing workflow status (goal, state, session) — TUI-equivalent summary for the web terminal.
 */
export function SessionRuntimeStatusBar({ statusLine }: SessionRuntimeStatusBarProps) {
  return (
    <div
      data-testid="session-runtime-status"
      role="status"
      aria-live="polite"
      style={{
        flexShrink: 0,
        minHeight: 28,
        padding: "6px 10px",
        fontSize: 12,
        lineHeight: 1.3,
        fontFamily: "ui-monospace, monospace",
        backgroundColor: "#1a1a1a",
        color: "#c8c8c8",
        borderBottom: "1px solid #333",
        overflow: "hidden",
        textOverflow: "ellipsis",
        whiteSpace: "nowrap",
      }}
    >
      {statusLine}
    </div>
  );
}
