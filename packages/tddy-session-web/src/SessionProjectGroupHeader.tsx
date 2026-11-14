import type React from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { ProjectSessionGroup } from "./utils/sessionProjectGroups";

export interface SessionProjectGroupHeaderProps {
  group: ProjectSessionGroup;
  /** The group's key in test ids (its project id, or `"other"`). */
  groupKey: string;
  collapsed: boolean;
  onToggle: () => void;
  /**
   * Opens the orchestrator composer for this project — the header's "+"
   * (`sessions-drawer-project-new-<projectId>`). Absent: no "+" (the **Other** group, or a drawer
   * that cannot start one).
   */
  onNew?: () => void;
  /** Further header actions after the "+" — the slot `#orchestrators` 11/11 puts its gear in. */
  actions?: React.ReactNode;
}

/**
 * A project group's header row (`#orchestrators` 8/11): the collapse toggle — the project's name
 * and session count, `sessions-drawer-project-group-header-<key>` — then the "+" and any further
 * actions as siblings, since a button cannot hold buttons.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-orchestrator-composer.md
 */
export function SessionProjectGroupHeader(props: SessionProjectGroupHeaderProps): React.ReactElement {
  const { group, groupKey, collapsed, onToggle, onNew, actions } = props;
  const Chevron = collapsed ? ChevronRight : ChevronDown;
  return (
    <div className="flex w-full items-center gap-1">
      <button
        type="button"
        data-testid={`sessions-drawer-project-group-header-${groupKey}`}
        aria-expanded={!collapsed}
        onClick={onToggle}
        className="flex flex-1 min-w-0 items-center gap-1 px-1 py-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground transition-colors hover:text-foreground"
      >
        <Chevron className="h-3.5 w-3.5 flex-shrink-0" />
        <span className="flex-1 truncate text-left">{group.label}</span>
        <span className="font-normal tabular-nums">{group.sessions.length}</span>
      </button>
      {onNew && (
        <button
          type="button"
          data-testid={`sessions-drawer-project-new-${groupKey}`}
          onClick={onNew}
          className="px-1 py-1 text-xs text-muted-foreground transition-colors hover:text-foreground"
          title={`New orchestrator for ${group.label}`}
          aria-label={`New orchestrator for ${group.label}`}
        >
          +
        </button>
      )}
      {actions}
    </div>
  );
}
