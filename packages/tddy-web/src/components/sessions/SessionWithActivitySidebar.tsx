import type React from "react";
import type { Client } from "@connectrpc/connect";
import type { ActivityService } from "../../gen/activity_pb";
import type { SessionEntry } from "../../gen/session_pb";

/**
 * The session view with the Agent Activity transcript docked as a **right sidebar**
 * (`#project-home` 3/7), replacing the top-bar popover.
 *
 * - `toolbar` is where the toggle (with its unread badge) is rendered — the main pane's header row.
 * - On selecting a session the sidebar's open state is `defaultActivitySidebarOpen(session)`; a
 *   toggle applies to that session until another one is selected.
 * - `suppressed` (the main pane already shows the Activities view) renders no sidebar and no
 *   toggle: one transcript per pane.
 * - At mobile width the sidebar is a full-width overlay instead of a docked column.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-22-project-home-activity-sidebar.md.
 */
export function SessionWithActivitySidebar(props: {
  session: SessionEntry;
  sessionToken: string;
  client?: Client<typeof ActivityService>;
  suppressed: boolean;
  isMobile?: boolean;
  toolbar: (toggle: React.ReactNode) => React.ReactNode;
  children: React.ReactNode;
}): React.ReactElement {
  // TODO(activity-sidebar): implement
  return <>{props.children}</>;
}
