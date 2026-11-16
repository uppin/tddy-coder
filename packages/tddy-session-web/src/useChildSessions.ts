import { useMemo } from "react";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import { ORCHESTRATOR_ROLE, PROJECT_HOME_ROLE } from "./utils/projectHomeGroups";

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
 * A project-home or orchestrator parent has no child tabs: every session it started through
 * `StartSession` records it as orchestrator, and those are started sessions, not the spawned
 * conversations the tabs were designed for.
 *
 * Feature: `docs/ft/coder/spawn-conversation.md`.
 */
export function useChildSessions(
  parentSessionId: string,
  sessions: ReadonlyArray<SessionEntry>,
): ChildSession[] {
  return useMemo(() => {
    const parent = sessions.find((s) => s.sessionId === parentSessionId);
    if (
      parent?.sessionRole === PROJECT_HOME_ROLE ||
      parent?.sessionRole === ORCHESTRATOR_ROLE
    ) {
      return [];
    }
    return sessions
      .filter(
        (s) =>
          s.orchestratorSessionId === parentSessionId && s.sessionId !== parentSessionId,
      )
      .map((s) => ({ sessionId: s.sessionId, recipe: s.recipe }));
  }, [sessions, parentSessionId]);
}
