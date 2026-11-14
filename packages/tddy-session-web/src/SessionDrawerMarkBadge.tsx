import type { SessionEntry } from "tddy-client-web/gen/session_pb";

/**
 * The mark an orchestrator put on a session — `paused`, `blocked` or `done` — as a badge on its
 * drawer row (`sessions-drawer-item-mark-<id>`), the orchestrator's note as its hover text
 * (`#orchestrators` 5/11). Renders nothing for an unmarked session.
 *
 * Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
 */
export function SessionDrawerMarkBadge({ session }: { session: SessionEntry }) {
  const mark = session.orchestrationMark;
  if (!mark?.mark) {
    return null;
  }
  return (
    <span
      data-testid={`sessions-drawer-item-mark-${session.sessionId}`}
      title={mark.note || undefined}
      className="flex-shrink-0 text-[10px] leading-none px-1.5 py-0.5 rounded bg-muted text-muted-foreground"
    >
      {mark.mark}
    </span>
  );
}
