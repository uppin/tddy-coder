# `session_agents.SessionAgentService` — the ten methods and their ports

## Role

`tddy-session-agents` serves the ten RPCs of family B: who is attached to a session's agent roster,
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
| `ResumeAgentConversation` | server-stream | the same, for a turn that asks nothing new — optionally after a rewind and a correction |
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

Eight of the ten route on the `daemon_instance_id` the **request** names — a roster lives on the
daemon facilitating its session, so a call served anywhere else answers about the wrong host. That
fork needs the eligible-daemon roster, the common room slot and the LiveKit forwarding clients, so
it lives in `tddy-session-lifecycle`'s `PeerRoutedSessionAgents`
([`connection_service/svc_session_agent_ports/svc_peer_routed_session_agents.rs`](../../tddy-session-lifecycle/src/connection_service/svc_session_agent_ports/svc_peer_routed_session_agents.rs)),
which wraps this crate's implementation rather than the crate growing a transport.

The forward that follows the **agent's** owning daemon is a different decision and *is* made here:
it can only be taken once the roster entry naming the owner has been read, which is this crate's
read. Delivering it is `AgentConversationPeers`'.

## Six of these ten are a security boundary

`packages/tddy-sandbox-runner/src/runner.rs` holds the `(service, method)` allowlist of what an
in-jail agent may relay to its host. Six entries are family B: `StreamSessionAgents`,
`OpenAgentConversation`, `PromptAgentConversation`, `ResumeAgentConversation`,
`CancelAgentConversation` and `ReportAgentConversationState`.

`ResumeAgentConversation` is in the set because a conversation an in-jail `tddy-tools` opened
over this relay is one it must also be able to continue: without the entry a jailed conversation
could be started and never carried on. It reaches the same code path as `PromptAgentConversation`
under the same authentication, so it opens no route weaker than one already open — and the
justification stops there. It is **not** the case that a caller is confined to its own session's
conversations: `SessionAgentServiceImpl::session_dir` resolves the token to an OS user and never
cross-checks `session_id` against it, and `OpenAgentConversations::routing_for` reads a
host-global map keyed on conversation id alone. Pre-existing — `Prompt` and `Cancel` were already
relayable through it — and recorded in
[`docs/dev/todo/2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md`](../../../docs/dev/todo/2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md).
What resume adds over prompt is a **destructive write** rather than one more turn:
`from_message_id` truncates a transcript and `correction` injects into it.

The pairs are **data, not literals**:
[`tddy_service::session_agents::IN_JAIL_RELAYABLE`](../../tddy-service/src/session_agents.rs) is the
one declaration, read by the runner, by this crate and by nothing else. It is declared in
`tddy-service` rather than here because its second reader is the binary that runs *inside every
jail*, which must not link this crate's `livekit` dependency tree for five string pairs.

The permitted operation set is pinned by a unit test in `src/lib.rs`, which is what keeps each
widening deliberate: `#unbundle` node 7 changed the service name each pair carries and nothing
else, because widening or narrowing the set inside a mechanical move is exactly what a stack like
that makes easy and must not do.

Moving the coordinate without moving the allowlist would make every in-jail subagent conversation
fail **closed** — the safe direction, but silently and at runtime rather than at compile time. That
is why the acceptance test drives a real jail rather than reading the list: a test that read the
tuples would agree with the runner by construction and prove nothing about the name.

## What a conversation turn carries, in both directions

A jail holds no agent definition, so in a jailed deployment every subagent conversation is served
here rather than run in the caller's process. Everything a caller can say about a turn, and
everything a turn can report about itself, is therefore a field on this coordinate:

| Field | Message | Meaning |
|---|---|---|
| `max_turns` | `PromptAgentConversationRequest`, `ResumeAgentConversationRequest` | the budget for this one call, in place of the definition's. Clamped to the serving host's bounds |
| `from_message_id` | `ResumeAgentConversationRequest` | rewind to a message the conversation holds, discarding what follows |
| `correction` | `ResumeAgentConversationRequest` | one corrective instruction appended after the rewind point |
| `messages` | `AgentConversationChunk`, **final frame** | `AgentMessageDescriptor { id, role, tool, tool_calls, is_error, preview }` per message the turn appended |
| `clamped_max_turns` | `AgentConversationChunk`, **final frame** | the budget actually applied; unset when the caller got what it asked for |

`messages` and `clamped_max_turns` ride the final frame for the reason `stop_reason` does: neither
is known until the turn has ended. `preview` is cut by the host that owns the history, because a
turn outcome carrying whole tool payloads would put the agent's context back into its caller's.

## Known gaps

- **`in_jail_conversation_acceptance.rs` proves nothing today.** It carries
  `#![cfg(target_os = "macos")]` as an inner attribute, so on Ubuntu CI it compiles to an empty test
  binary and runs none of its cases; on macOS it fails in setup on a stdio-bridge defect its own
  `FIXME(sandbox-stdio-attach)` records, which is a predecessor's, not family B's. See
  [`docs/dev/todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md`](../../../docs/dev/todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md).
- **`ResumeAgentConversation` is not proven across the wire.** It is compile-checked and clippy
  clean, and covered on either side of this coordinate — `tddy-discovery`'s local-session tests for
  the turn loop, `tddy-tools`' `--mcp` stdio acceptance tests for the tool shape — but no test
  drives the RPC itself end to end. The suite that would is the one directly above. Recorded in
  [`docs/dev/todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md`](../../../docs/dev/todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md);
  a long `messages` list on the final frame can also overflow the chunk-framing threshold
  ([`2026-09-26`](../../../docs/dev/todo/2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md)).
- **`service.rs` is 1,055 production lines**, over this repo's 500-line budget, and was not split. See
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
- [`tddy-session-lifecycle` session-service.md](../../tddy-session-lifecycle/docs/session-service.md) — the session RPC host this crate stayed beside
