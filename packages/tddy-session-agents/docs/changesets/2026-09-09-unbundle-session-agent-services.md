# 2026-09-09 — The crate, and the nine methods it serves

`tddy-session-agents` serves `session_agents.SessionAgentService`: a session's agent roster, and the
conversations held with the agents on it. Four modules moved out of `tddy-daemon`
(`session_agent_roster`, `session_agent_status`, `session_agent_clone`, `session_agent_inference`),
joined by three written here — `service` (the nine handlers), `ports` (what only a daemon can answer)
and `agent_conversations` (the map of open conversations). 47 tests.

Every handler resolves its session directory from the caller's own token through
`SessionDirResolver` — a closure, not a base path, because the token→OS-user mapping and the
one-path-segment validation are both the daemon's, and a caller holding a valid token for one of its
own sessions must not be able to name a roster it does not own.

`conversations` is an injected `Arc`, not state this crate creates. The daemon dispatches a local
agent's own tool calls against the same map, outside any RPC on this coordinate; two maps would have
a prompt answer `NOT_FOUND` for a conversation an open had just created.

Peer routing on the **request's** `daemon_instance_id` is deliberately not here — `tddy-daemon`'s
`PeerRoutedSessionAgents` wraps this implementation, because seven of the nine route and that needs
the eligible-daemon roster, the common room slot and the LiveKit forwarding clients. The forward that
follows the **agent's** owning daemon *is* here: it can only be taken after reading the roster entry
that names the owner, which is this crate's read.

`IN_JAIL_RELAYABLE` — the five `(service, method)` pairs an in-jail agent may relay to its host — is
re-exported rather than declared, because its second reader is `tddy-sandbox-runner`, the binary that
runs inside every jail and must not link this crate's `livekit` dependencies for five string pairs.
The permitted set is unchanged by the move and is pinned by two unit tests here.

`session_agent_inference` travelled with the crate but is read only by `ListSessions`, which stayed
in the daemon. It moved because it shares `session_agent_status`'s truncation rule, not because
anything on this coordinate calls it.

Two things recorded rather than fixed: `session_agent_clone.rs` (1,157 prod lines) holds both the
facilitating and the owning daemon's halves, and its owning half is the sole reason this crate
depends on `livekit` — contradicting `ports.rs`'s own header. And `service.rs` is 876 prod lines,
over budget.

Docs: [session-agent-service.md](../session-agent-service.md),
[session-agent-roster.md](../session-agent-roster.md),
[agent-session-status.md](../agent-session-status.md).
Full record: [../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md).
