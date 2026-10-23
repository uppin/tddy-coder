import type { SessionEntry } from "../../gen/session_pb";

/**
 * Whether the Agent Activity sidebar starts open when `session` is selected (`#project-home` 3/7):
 * open for a managed-codebase session — its agent works through daemon tools, so the transcript is
 * the readable view of what it does — closed for every other session.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-22-project-home-activity-sidebar.md.
 */
export function defaultActivitySidebarOpen(session: Pick<SessionEntry, "managedCodebase">): boolean {
  return session.managedCodebase;
}
