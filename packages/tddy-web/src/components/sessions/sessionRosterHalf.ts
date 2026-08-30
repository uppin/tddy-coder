import type { SessionEntry } from "../../gen/connection_pb";

/** Which session, on which daemon, a roster call names. */
export interface RosterHalf {
  readonly sessionId: string;
  readonly daemonInstanceId: string;
}

/**
 * The half of a session whose roster governs its agent.
 *
 * A split session is two sessions — the agent runs on one host, the worktree and codebase live on
 * another — and only the codebase half keeps the roster. That is the copy the agent's own tooling
 * subscribes to: a split agent is launched pointed at `codebase_session_id` on
 * `codebase_daemon_instance_id` (`split_session::split_remote_tool_env`), and its withdrawn tools
 * are derived from the roster held there. The agent half would answer a roster read too, with an
 * empty list beside the real one — and an attach made against it would report a withdrawal no
 * process performs.
 *
 * Both fields are written together or not at all, so a session with neither is co-located and
 * addresses itself.
 *
 * Applied per node of the Agents tab's tree, not once: a subagent session can be split
 * independently of the session that spawned it.
 */
export function rosterHalfOf(session: SessionEntry): RosterHalf {
  const codebaseHost = session.codebaseDaemonInstanceId.trim();
  const codebaseSession = session.codebaseSessionId.trim();
  if (codebaseHost === "" || codebaseSession === "") {
    return { sessionId: session.sessionId, daemonInstanceId: session.daemonInstanceId };
  }
  return { sessionId: codebaseSession, daemonInstanceId: codebaseHost };
}
