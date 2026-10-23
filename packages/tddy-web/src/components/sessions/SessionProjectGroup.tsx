import type React from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { ProjectSessionGroup } from "../../utils/sessionProjectGroups";

/** The key a group is known by in test ids and the collapsed set: its project id, or `"other"`. */
export function projectGroupKey(group: ProjectSessionGroup): string {
  return group.projectId ?? "other";
}

/**
 * One project group in the expanded drawer: a header with the project's name and session count
 * that collapses or expands the group, and the group's body (`children`) — today's active/remaining
 * partition and stack nesting, over this group's sessions only.
 *
 * A collapsed body stays mounted and is hidden with `display: none`, the same way the drawer's
 * partition separators hide theirs.
 *
 * PRD: docs/ft/web/session-drawer.md.
 */
export function SessionProjectGroup(props: {
  group: ProjectSessionGroup;
  collapsed: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}): React.ReactElement | null {
  const { group, collapsed, onToggle, children } = props;
  const key = projectGroupKey(group);
  const Chevron = collapsed ? ChevronRight : ChevronDown;

  return (
    <div data-testid={`sessions-drawer-project-group-${key}`}>
      <button
        type="button"
        data-testid={`sessions-drawer-project-group-header-${key}`}
        aria-expanded={!collapsed}
        onClick={onToggle}
        className="flex w-full items-center gap-1 px-1 py-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground transition-colors hover:text-foreground"
      >
        <Chevron className="h-3.5 w-3.5 flex-shrink-0" />
        <span className="flex-1 truncate text-left">{group.label}</span>
        <span className="font-normal tabular-nums">{group.sessions.length}</span>
      </button>
      <div style={collapsed ? { display: "none" } : undefined}>{children}</div>
    </div>
  );
}
