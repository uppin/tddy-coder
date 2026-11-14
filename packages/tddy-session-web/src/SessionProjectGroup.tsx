import type React from "react";
import type { ProjectSessionGroup } from "./utils/sessionProjectGroups";
import { SessionProjectGroupHeader } from "./SessionProjectGroupHeader";

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
  /** Opens the orchestrator composer for this group's project — the header's "+". */
  onNew?: () => void;
  children: React.ReactNode;
}): React.ReactElement | null {
  const { group, collapsed, onToggle, onNew, children } = props;
  const key = projectGroupKey(group);

  return (
    <div data-testid={`sessions-drawer-project-group-${key}`}>
      <SessionProjectGroupHeader
        group={group}
        groupKey={key}
        collapsed={collapsed}
        onToggle={onToggle}
        onNew={onNew}
      />
      <div style={collapsed ? { display: "none" } : undefined}>{children}</div>
    </div>
  );
}
