import React from "react";

export type InspectorTab = "details" | "tools" | "usage" | "worktree" | "vnc" | "screen-sharing";

interface InspectorTabsProps {
  value: InspectorTab;
  onChange: (tab: InspectorTab) => void;
}

export function InspectorTabs({ value, onChange }: InspectorTabsProps) {
  return (
    <div className="flex border-b border-border flex-shrink-0">
      <button
        data-testid="sessions-inspector-tab-details"
        aria-selected={value === "details"}
        onClick={() => onChange("details")}
        className={`px-3 py-1.5 text-xs font-medium border-b-2 transition-colors ${
          value === "details"
            ? "border-foreground text-foreground"
            : "border-transparent text-muted-foreground hover:text-foreground"
        }`}
      >
        Details
      </button>
      <button
        data-testid="sessions-inspector-tab-tools"
        aria-selected={value === "tools"}
        onClick={() => onChange("tools")}
        className={`px-3 py-1.5 text-xs font-medium border-b-2 transition-colors ${
          value === "tools"
            ? "border-foreground text-foreground"
            : "border-transparent text-muted-foreground hover:text-foreground"
        }`}
      >
        Tools
      </button>
      <button
        data-testid="sessions-inspector-tab-usage"
        aria-selected={value === "usage"}
        onClick={() => onChange("usage")}
        className={`px-3 py-1.5 text-xs font-medium border-b-2 transition-colors ${
          value === "usage"
            ? "border-foreground text-foreground"
            : "border-transparent text-muted-foreground hover:text-foreground"
        }`}
      >
        Usage
      </button>
      <button
        data-testid="sessions-inspector-tab-worktree"
        aria-selected={value === "worktree"}
        onClick={() => onChange("worktree")}
        className={`px-3 py-1.5 text-xs font-medium border-b-2 transition-colors ${
          value === "worktree"
            ? "border-foreground text-foreground"
            : "border-transparent text-muted-foreground hover:text-foreground"
        }`}
      >
        Worktree
      </button>
      <button
        data-testid="sessions-inspector-tab-vnc"
        aria-selected={value === "vnc"}
        onClick={() => onChange("vnc")}
        className={`px-3 py-1.5 text-xs font-medium border-b-2 transition-colors ${
          value === "vnc"
            ? "border-foreground text-foreground"
            : "border-transparent text-muted-foreground hover:text-foreground"
        }`}
      >
        VNC
      </button>
      <button
        data-testid="sessions-inspector-tab-screen-sharing"
        aria-selected={value === "screen-sharing"}
        onClick={() => onChange("screen-sharing")}
        className={`px-3 py-1.5 text-xs font-medium border-b-2 transition-colors ${
          value === "screen-sharing"
            ? "border-foreground text-foreground"
            : "border-transparent text-muted-foreground hover:text-foreground"
        }`}
      >
        Screen Sharing
      </button>
    </div>
  );
}
