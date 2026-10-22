import type React from "react";
import type { ProjectSessionGroup } from "../../utils/sessionProjectGroups";

/**
 * One project group in the expanded drawer: a header with the project's name and session count
 * that collapses or expands the group, and the group's body (`children`) — today's active/remaining
 * partition and stack nesting, over this group's sessions only.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-22-project-home-project-groups.md.
 */
export function SessionProjectGroup(props: {
  group: ProjectSessionGroup;
  collapsed: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}): React.ReactElement | null {
  // TODO(project-groups): implement
  void props;
  return null;
}
