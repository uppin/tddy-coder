import { useMemo } from "react";
import type { SessionEntry } from "../../gen/connection_pb";

/** A spawned child conversation of a parent session — enough to render its tab and attach it. */
export interface ChildSession {
  readonly sessionId: string;
  /** The recipe the child conversation runs (e.g. `grill-me`); used for the tab label. */
  readonly recipe: string;
}

/**
 * Derive the child conversations spawned by a parent session from the drawer's session list.
 *
 * A child session is any `SessionEntry` tagged with `orchestratorSessionId === parentSessionId`
 * (the daemon sets this when a workflow calls `spawn_conversation`). The parent itself is excluded
 * defensively so a session can never list itself as its own child.
 *
 * Feature: `docs/ft/coder/spawn-conversation.md`.
 */
export function useChildSessions(
  parentSessionId: string,
  sessions: ReadonlyArray<SessionEntry>,
): ChildSession[] {
  return useMemo(
    () =>
      sessions
        .filter(
          (s) =>
            s.orchestratorSessionId === parentSessionId && s.sessionId !== parentSessionId,
        )
        .map((s) => ({ sessionId: s.sessionId, recipe: s.recipe })),
    [sessions, parentSessionId],
  );
}
