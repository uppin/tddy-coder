import type { SessionEntry } from "../../gen/session_pb";

/**
 * The sessions to show, with the codebase halves of sandboxed-codebase placements removed.
 *
 * A `sandboxed_codebase` session is served by **two** sessions: the `claude-cli` agent, and the
 * `workspace` session whose jail holds the checkout (`svc_start_sandboxed_codebase_session` mints
 * both). On a *split* placement those live on two different daemons, so a host's list only ever
 * held one of them. This placement is the first to put both on ONE daemon, so `ListSessions`
 * returns both and the drawer grew a second row per session — one the operator never started, has
 * no agent or terminal, and cannot resume. The PRD asks for the opposite: one row, rendering the
 * placement as agent host and codebase host
 * (docs/ft/daemon/remote-managed-worktree.md § Sandboxed codebase placement).
 *
 * The half is identified by the claim its agent already carries: `codebase_session_id`. The
 * `.session.yaml` back-pointer (`agent_session_id`) would say the same thing from the other side
 * and is the more direct question to ask, but it is not on `SessionEntry` — adding it is a proto
 * change, and the claim reaching the page is enough to answer this one.
 *
 * A claimed half is dropped only when the agent claiming it is **in this list**. An unclaimed
 * `workspace` session is a session in its own right (a standalone workspace, an agent clone's
 * mirror) and stays, so nothing can vanish from the drawer merely by being of that type.
 */
export function withoutJailedCodebaseHalves(sessions: readonly SessionEntry[]): SessionEntry[] {
  const claimed = new Set(
    sessions.map((s) => s.codebaseSessionId.trim()).filter((id) => id.length > 0),
  );
  if (claimed.size === 0) return [...sessions];
  return sessions.filter((s) => !claimed.has(s.sessionId));
}
