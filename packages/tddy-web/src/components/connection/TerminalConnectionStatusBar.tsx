import React, { useEffect } from "react";
import { tddyDevDebug } from "../../lib/tddyDevLog";

export interface TerminalConnectionStatusBarProps {
  children?: React.ReactNode;
  className?: string;
  style?: React.CSSProperties;
}

/**
 * Dedicated top row above the Ghostty viewport (connection chrome, LiveKit consolidation).
 */
export function TerminalConnectionStatusBar({
  children,
  className,
  style,
}: TerminalConnectionStatusBarProps) {
  useEffect(() => {
    tddyDevDebug("[TerminalConnectionStatusBar] mount");
  }, []);

  return (
    <div
      data-testid="terminal-connection-status-bar"
      className={className}
      style={{
        flexShrink: 0,
        minHeight: 0,
        ...style,
      }}
      role="toolbar"
      aria-label="Terminal connection"
    >
      {children}
    </div>
  );
}
