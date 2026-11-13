import type { SessionEntry } from "tddy-client-web/gen/session_pb";

/**
 * The mark an orchestrator put on a session — `paused`, `blocked` or `done` — as a badge on its
 * drawer row (`sessions-drawer-item-mark-<id>`), the orchestrator's note as its hover text
 * (`#orchestrators` 5/11). Renders nothing for an unmarked session.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-session-lifecycle-tools.md
 */
export function SessionDrawerMarkBadge({ session }: { session: SessionEntry }) {
  // TODO(session-lifecycle-tools): implement — badge `session.orchestrationMark`, note on hover.
  void session;
  return null;
}
