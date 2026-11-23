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
  /**
   * The project's settings cog, shown left of the name while the pointer is anywhere over the
   * header row (the row is the hover target, so moving from the name onto the cog keeps it shown).
   */
  actions?: React.ReactNode;
}

/**
 * A project group's header row (`#orchestrators` 8/11): the settings cog (hover-revealed), the
 * collapse toggle — the project's name, `sessions-drawer-project-group-header-<key>` — then the "+",
 * all siblings since a button cannot hold buttons.
 *
 * Feature: docs/ft/web/session-drawer.md § Orchestrator composer
 */
export function SessionProjectGroupHeader(props: SessionProjectGroupHeaderProps): React.ReactElement {
  const { group, groupKey, collapsed, onToggle, onNew, actions } = props;
  const Chevron = collapsed ? ChevronRight : ChevronDown;
  return (
    <div
      data-testid={`sessions-drawer-project-header-row-${groupKey}`}
      className="group flex w-full items-center gap-1"
    >
      {actions}
      <button
        type="button"
        data-testid={`sessions-drawer-project-group-header-${groupKey}`}
        aria-expanded={!collapsed}
        onClick={onToggle}
        className="flex flex-1 min-w-0 items-center gap-1 px-1 py-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground transition-colors hover:text-foreground"
      >
        <Chevron className="h-3.5 w-3.5 flex-shrink-0" />
        <span className="flex-1 truncate text-left">{group.label}</span>
      </button>
      {onNew && (
        <button
          type="button"
          data-testid={`sessions-drawer-project-new-${groupKey}`}
          onClick={onNew}
          className="flex-shrink-0 px-1 py-1 text-xs text-muted-foreground transition-colors hover:text-foreground"
          title={`New orchestrator for ${group.label}`}
          aria-label={`New orchestrator for ${group.label}`}
        >
          +
        </button>
      )}
    </div>
  );
}
