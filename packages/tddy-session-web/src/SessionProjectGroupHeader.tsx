import type React from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { cn } from "tddy-ui-web";
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
 * A project group's header row (`#orchestrators` 8/11): the collapse toggle — the project's name,
 * `sessions-drawer-project-group-header-<key>`, starting where session rows' dots do — then the "+".
 * Left of the name, in the gutter, the chevron; hovering the row swaps it for the settings cog. All
 * siblings, since a button cannot hold buttons; the chevron toggles too.
 *
 * Feature: docs/ft/web/session-drawer.md § Orchestrator composer
 */
export function SessionProjectGroupHeader(props: SessionProjectGroupHeaderProps): React.ReactElement {
  const { group, groupKey, collapsed, onToggle, onNew, actions } = props;
  const Chevron = collapsed ? ChevronRight : ChevronDown;
  return (
    <div
      data-testid={`sessions-drawer-project-header-row-${groupKey}`}
      className="group relative flex w-full items-center gap-1"
    >
      {/* The name starts at 12px — a session row's dot (px-3). The 12px gutter before it holds the
          chevron, which the settings cog replaces while the row is hovered. */}
      <span className="absolute inset-y-0 left-0 flex w-3 items-center justify-center">
        <button
          type="button"
          tabIndex={-1}
          aria-hidden="true"
          onClick={onToggle}
          className={cn(
            "flex items-center text-muted-foreground transition-opacity",
            actions && "group-hover:opacity-0 group-focus-within:opacity-0",
          )}
        >
          <Chevron className="h-3 w-3" />
        </button>
        {actions && <span className="absolute inset-0 flex items-center justify-center">{actions}</span>}
      </span>
      <button
        type="button"
        data-testid={`sessions-drawer-project-group-header-${groupKey}`}
        aria-expanded={!collapsed}
        onClick={onToggle}
        className="flex flex-1 min-w-0 items-center py-1 pl-3 text-xs font-semibold uppercase tracking-wide text-muted-foreground transition-colors group-hover:text-foreground"
      >
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
