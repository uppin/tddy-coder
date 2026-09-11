# tddy-session-agents

The agents attached to a session, and the conversations held with them: `session_agents.SessionAgentService`,
nine RPCs. A roster agent is a specialized model loop the daemon serves *inside* one session,
addressable as `name@daemon_instance_id`, seeded at session start or attached while it runs — and it
may be owned by a different daemon than the one facilitating the session, in which case this crate
also owns the checkout that daemon cuts and the mirror that keeps it current.

Extracted from `tddy-daemon` by `#unbundle` node 7.

## Quick Start

### Testing
```bash
cargo test -p tddy-session-agents
```

47 unit tests, in three modules. The four acceptance suites that cover this subsystem live in
`packages/tddy-daemon/tests/` and stayed there: each is pinned by `ConnectionServiceImpl` or
`test_util::{test_service, TEST_TOKEN}`, and moving either would put `tddy-daemon` back on this
crate's dependency path.

## Architecture

Every handler resolves its session directory from the caller's own token through a port, never from
a path the request supplied — so `SessionAgentPorts` carries resolvers and capabilities rather than
values. What a daemon knows and this crate cannot is: which directory a token may reach, which def
an agent id resolves to, whether a checkout can be claimed on a peer, and how a roster change
reaches a room.

Peer routing on the request's `daemon_instance_id` is the daemon's (`PeerRoutedSessionAgents`); the
forward that follows the *agent's* owning daemon is made here, because it can only be taken after
reading the roster entry that names the owner.

**Five of the nine methods are a security boundary.** They are what an in-jail agent may relay to
its host, and the `(service, method)` pairs live once, in
`tddy_service::session_agents::IN_JAIL_RELAYABLE`, read by this crate and by `tddy-sandbox-runner`.
An allowlist that no longer matches the served coordinate fails **closed** — silently, at runtime.

## Documentation

### Product Requirements (What)
- [session-agent-roster.md](../../docs/ft/daemon/session-agent-roster.md) — the roster feature
- [specialized-subagents.md](../../docs/ft/coder/specialized-subagents.md) — where agent defs come from
- [agent-session-status.md](../../docs/ft/daemon/agent-session-status.md) — the *other* agent status, the one a session's own agent reports
- [remote-codebase-mode.md](../../docs/ft/daemon/remote-codebase-mode.md) — the jail an agent relays out of
- [remote-managed-worktree.md](../../docs/ft/daemon/remote-managed-worktree.md) — the workspace-session and tool-proxy primitives a clone reuses

### Technical Implementation (How)
- [session-agent-service.md](./docs/session-agent-service.md) — the nine methods, the ports, the routing split and the allowlist
- [session-agent-roster.md](./docs/session-agent-roster.md) — the roster store, the clone's two sides, the mirror and the status mapping
- [agent-session-status.md](./docs/agent-session-status.md) — `session_agent_inference`, which travels here but is read only by `ListSessions`

## Related Packages
- [tddy-service](../tddy-service/docs/) — owns `session_agents.proto`, `types.proto` and `IN_JAIL_RELAYABLE`
- [tddy-discovery](../tddy-discovery/docs/roster-and-subagent-runtime.md) — the subagent conversation runtime and `LiveAgentRoster` this crate serves in front of
- [tddy-daemon](../tddy-daemon/docs/connection-service.md) — the host: its ports, its peer routing, and `StartSession`, which seeds the roster
- [tddy-sandbox-runner](../tddy-sandbox-runner/docs/) — the in-jail relay that reads the allowlist
- [tddy-session-sync](../tddy-session-sync/docs/) — the mirror algorithm a clone runs in-process
- [tddy-daemon-kernel](../tddy-daemon-kernel/docs/) — publishes `AgentActivityHub` and `now_unix_ms`
