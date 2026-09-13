# `session_agents.SessionAgentService` — the nine methods and their ports

## Role

`tddy-session-agents` serves the nine RPCs of family B: who is attached to a session's agent roster,
and what has been asked of them. The coordinate and the crate both arrived with `#unbundle` node 7,
which moved the family off `connection.ConnectionService` (50 methods → 33) along with its four
source modules.

| Method | Kind | Answers about |
|---|---|---|
| `AttachSessionAgent` | unary | the roster after an agent joins it |
| `DetachSessionAgent` | unary | the roster after an agent leaves it |
| `ListSessionAgents` | unary | the roster now |
| `StreamSessionAgents` | server-stream | the roster now, and every revision after |
| `OpenAgentConversation` | unary | a conversation id, and where its turn loop runs |
| `PromptAgentConversation` | server-stream | one turn's frames, local or relayed |
| `CancelAgentConversation` | unary | a turn interrupted and a conversation closed |
| `ReportAgentCloneState` | unary | a checkout's readiness, pushed by the daemon holding it |
| `ReportAgentConversationState` | unary | what a jailed agent says it is doing, clamped |

Proto: [`packages/tddy-service/proto/session_agents.proto`](../../tddy-service/proto/session_agents.proto).
It imports `types.proto` for `SessionAgentEntry`, `SessionAgentRoster`, `SessionAgentStatus`,
`SessionAgentActivity` and `StreamMode` — the last four because `ListSessions`, which is family C
and stays in the daemon, reaches them through `SessionEntry`.

## Every method starts by resolving the session directory from the caller's own token

`SessionDirResolver` is a closure the daemon supplies, not a base path this crate derives, because
both halves of the answer are the daemon's: a token maps to an OS user through the `users[]` table
in its config, and a session id has to be validated as one path segment before it becomes one. A
caller holding a valid token for one of its own sessions must not be able to name a roster it does
not own, and only the side holding the mapping can refuse that.

## The ports, and why each is one

`SessionAgentPorts` is a struct rather than a dozen positional parameters, following
`tddy_session_files::SessionFilesPorts`, which node 6 established for the same reason. Every field
carries why the daemon supplies it.

| Port | What only a daemon can answer |
|---|---|
| `session_dirs` | which directory a session token may reach on this host |
| `local_instance_id` | whether an agent is *this* host's — the one fact that decides whether a checkout is claimed and whether a turn loop runs here |
| `roster_keepalive` | the cadence an unchanged roster is re-sent at; a tuning of the sending host |
| `rosters`, `clones`, `conversations` | shared `Arc`s, not owned state: `ListSessions` reports the same rows, a seeded start writes the same map, and the daemon dispatches a local agent's own tool calls against the same conversation map outside any RPC on this coordinate |
| `admission` | whether a checkout could be claimed on the peer owning an agent |
| `catalog` | which def an agent id resolves to on this host |
| `broadcast` | how a roster change reaches a session room |
| `sessions` | how a turn loop is opened against a jail or a clone |
| `peers` | how a conversation is delivered to the daemon running it |

Two maps would be the failure this shape exists to prevent: a prompt answering `NOT_FOUND` for a
conversation an open had just created.

## The routing split: two forwards, one of them not here

Seven of the nine route on the `daemon_instance_id` the **request** names — a roster lives on the
daemon facilitating its session, so a call served anywhere else answers about the wrong host. That
fork needs the eligible-daemon roster, the common room slot and the LiveKit forwarding clients, so
it lives in `tddy-daemon`'s `PeerRoutedSessionAgents`
([`connection_service/svc_session_agent_ports.rs`](../../tddy-daemon/src/connection_service/svc_session_agent_ports.rs)),
which wraps this crate's implementation rather than the crate growing a transport.

The forward that follows the **agent's** owning daemon is a different decision and *is* made here:
it can only be taken once the roster entry naming the owner has been read, which is this crate's
read. Delivering it is `AgentConversationPeers`'.

## Five of these nine are a security boundary

`packages/tddy-sandbox-runner/src/runner.rs` holds the `(service, method)` allowlist of what an
in-jail agent may relay to its host. Five entries are family B: `StreamSessionAgents`,
`OpenAgentConversation`, `PromptAgentConversation`, `CancelAgentConversation` and
`ReportAgentConversationState`.

The pairs are **data, not literals**:
[`tddy_service::session_agents::IN_JAIL_RELAYABLE`](../../tddy-service/src/session_agents.rs) is the
one declaration, read by the runner, by this crate and by nothing else. It is declared in
`tddy-service` rather than here because its second reader is the binary that runs *inside every
jail*, which must not link this crate's `livekit` dependency tree for five string pairs.

`#unbundle` node 7 changed the service name each pair carries and **nothing else**. The permitted
operation set is identical, pinned by a unit test in `src/lib.rs`. Widening or narrowing it inside a
mechanical move is exactly what a stack like this makes easy and must not do.

Moving the coordinate without moving the allowlist would make every in-jail subagent conversation
fail **closed** — the safe direction, but silently and at runtime rather than at compile time. That
is why the acceptance test drives a real jail rather than reading the list: a test that read the
tuples would agree with the runner by construction and prove nothing about the name.

## Known gaps

- **`in_jail_conversation_acceptance.rs` proves nothing today.** It carries
  `#![cfg(target_os = "macos")]` as an inner attribute, so on Ubuntu CI it compiles to an empty test
  binary and runs none of its cases; on macOS it fails in setup on a stdio-bridge defect its own
  `FIXME(sandbox-stdio-attach)` records, which is a predecessor's, not family B's. See
  [`docs/dev/todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md`](../../../docs/dev/todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md).
- **`service.rs` is 876 production lines**, over this repo's 500-line budget, and was not split. See
  [`docs/dev/todo/2026-09-12-the-two-new-service-rs-files-are-over-budget.md`](../../../docs/dev/todo/2026-09-12-the-two-new-service-rs-files-are-over-budget.md).
- **`session_agent_clone.rs` is 1,157 production lines** and is the sole reason this crate depends on
  `tddy-daemon-livekit` and `livekit` — which contradicts `ports.rs`'s own header claim that nothing
  here reaches for a peer or a room. See
  [`docs/dev/todo/2026-09-12-session-agent-clone-is-two-daemons-in-one-module.md`](../../../docs/dev/todo/2026-09-12-session-agent-clone-is-two-daemons-in-one-module.md).

## Related

- [session-agent-roster.md](./session-agent-roster.md) — the roster, clone and status modules underneath
- [agent-session-status.md](./agent-session-status.md) — the *other* meaning of agent status, which travels with this crate but is read only by `ListSessions`
- [docs/ft/daemon/session-agent-roster.md](../../../docs/ft/daemon/session-agent-roster.md) — the feature
- [docs/ft/coder/specialized-subagents.md](../../../docs/ft/coder/specialized-subagents.md) — where the agent defs come from
- [tddy-discovery](../../tddy-discovery/docs/roster-and-subagent-runtime.md) — the conversation runtime this crate serves in front of
- [connection-service.md](../../tddy-daemon/docs/connection-service.md) — the 33 methods that stayed
