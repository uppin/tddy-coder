# Session agent roster modules (`tddy_daemon::session_agent_roster`, `::session_agent_clone`)

## Role

Two modules implement a session's **agent roster** — the set of specialized agents attached to a
session, addressable as `name@daemon_instance_id`, seeded at start or mutated while the session
runs. See
[docs/ft/daemon/session-agent-roster.md](../../../docs/ft/daemon/session-agent-roster.md) for the
feature and its acceptance criteria.

| Module | Owns |
|---|---|
| `session_agent_roster` | The roster itself: membership, `rev`, persistence, subscriptions. |
| `session_agent_clone` | Everything about a **remote** agent: the checkout on its owning daemon, the mirror that keeps it current, and the hosted-side tool split. |

A start-time seed (`StartSessionRequest.specialized_agents[]`) is a caller of these same two
modules, not a parallel path: it resolves each reference through `roster_record_for`, claims a clone
through `session_agent_clone` for every agent not co-located with the authoritative worktree, and
writes the entries before the agent is spawned. See
[connection-service.md § Seeding the roster at start](connection-service.md).

The RPCs, the conversation routing and the room wiring live in `connection_service.rs` — there is
deliberately no `session_agent_conversation.rs`, because routing a conversation needs the peer
plumbing, the room handle and the roster in one place.

## `session_agent_roster::SessionAgentRosterStore`

```rust
pub fn new(clones: Arc<SessionAgentCloneStore>, activity: Arc<SessionAgentActivityStore>) -> Self
pub fn activity(&self) -> &Arc<SessionAgentActivityStore>
pub fn snapshot(&self, session_id, session_dir)      -> Result<SessionAgentRoster, Status>
pub fn attach(&self, session_id, session_dir, record) -> Result<SessionAgentRoster, Status>
pub fn detach(&self, session_id, session_dir, agent_id) -> Result<SessionAgentRoster, Status>
pub fn entry(&self, session_id, session_dir, agent_id) -> Result<SessionAgentEntry, Status>
pub fn agents_owned_by(&self, session_id, session_dir, daemon) -> Result<Vec<..>, Status>
pub fn subscribe(&self, session_id, session_dir)     -> Result<(snapshot, Receiver), Status>
pub fn republish(&self, session_id, session_dir)     -> Result<(), Status>
```

It holds **two** live stores because a roster entry is a projection of three things: the persisted
record, the clone state only the clone store knows, and what the agent is doing, which only the
activity store knows. `roster_entry` reads all three, which is why an entry's `clone_state` and
`status` are current without the roster having to be rewritten every time either advances.

The two live stores are passed together as `SnapshotSources { clones, activity }`, so neither can be
consulted without the other — an entry carrying a fresh `clone_state` and a stale `status` is two
different accounts of one agent.

Four properties are load-bearing and each is pinned by a test in
`tests/session_agent_roster_acceptance.rs`:

- **`attach` is idempotent on `agent_id`.** Re-attaching returns the snapshot, does not bump `rev`,
  and publishes nothing — an operator double-clicking *Add* must not push a frame to every
  subscriber for a change that did not happen.
- **`detach` of an absent id is `NOT_FOUND`.** A silent success would tell an operator a tool was
  restored to the main agent when it never was.
- **`subscribe` takes the snapshot and the receiver under one lock.** That is what makes
  snapshot-first correct rather than racy: a subscriber cannot miss a revision published between
  the two.
- **`rev` continues from disk.** Each session's roster is lazily rebuilt from `.session.yaml` on
  first use, so a restarted daemon does not restart at `rev: 1` — a subscriber holding `rev: 3`
  would read that as stale and never refresh.

A fifth is pinned in `session_agent_status.rs` and in `roster_entry`'s own tests:

- **A status is never persisted, and `UNSPECIFIED` is not `IDLE`.** Written to `.session.yaml` and
  read back, a status would claim a turn is in flight in a process that never started one. So the
  activity store is memory-only and a restarted daemon reports `UNSPECIFIED` — *"this daemon has
  nothing to say"* — until a signal reaches it.

`commit()` **persists then adopts**: a failed write leaves the in-memory store agreeing with the
file it will be rebuilt from, so a restart never silently undoes an attach the operator was told
succeeded. Persistence goes through `tddy_core::write_session_metadata` →
`atomic_file::write_atomic_labelled`.

## `session_agent_status`

The status half of an entry: what an agent is doing, and the mapping that decides it.

```rust
pub enum ManagedAgentState { NoConversation, Open, Prompting, ExecutingTool, WaitingForInput }
pub fn agent_status(clone_state: AgentCloneState, activity: Option<&AgentActivity>) -> SessionAgentStatus
pub fn reported_state(status: SessionAgentStatus) -> Option<ManagedAgentState>
pub fn tool_call_summary(tool_name: &str, args: &serde_json::Value) -> String

pub struct SessionAgentActivityStore { /* keyed by (session_id, agent_id) */ }
impl SessionAgentActivityStore {
    pub fn record(&self, session_id, agent_id, state, summary)
    pub fn record_turn_end(&self, session_id, agent_id, summary) -> bool
    pub fn get(&self, session_id, agent_id) -> Option<AgentActivity>
    pub fn forget(&self, session_id, agent_id)
    pub fn forget_session(&self, session_id)
}
```

Four decisions carry the module:

- **`agent_status` reads the clone first, and it wins outright.** An agent whose checkout is still
  provisioning refuses prompts, so `IDLE` would offer an operator an agent that cannot answer.
  `AgentCloneState::Unspecified` maps to `CONNECTING` for the same reason `roster_entry` refuses to
  call it `READY`.
- **Keyed by the pair, never `agent_id` alone.** One def attached to two sessions is one `agent_id`;
  keying on it would show a turn on one session as a turn on the other.
- **`record_turn_end` is guarded.** A turn's end is observed from a spawned task that outlives the
  handler, and by then a cancel or a detach may already have moved the agent on. An unconditional
  write would resurrect a conversation that is gone. It applies only from `Prompting`/`ExecutingTool`
  and returns whether anything changed, so the caller can skip republishing a roster that did not
  move.
- **`reported_state` clamps what a reporter may claim.** `ReportAgentConversationState` accepts only
  the four conversation states: `CONNECTING`/`ERROR` are the checkout's, which this daemon measures
  itself, and a reporter allowed to send them could hide a broken clone behind a cheerful
  conversation.

`tool_call_summary` carries a tool's name plus only the argument naming what it acted on, and
`record` truncates every summary to 120 **characters** (a cut mid-codepoint panics) collapsed to one
line. A `Write`'s `content` is the whole file, and a snapshot past `MAX_CHUNK_FRAME_BYTES` is
chunk-framed, where one lost frame wedges the call with no error.

### Who writes to it

`connection_service.rs` — `note_agent_activity` records and republishes together, because a status
change does not move `rev` and a subscriber that heard only about `rev` changes would show the state
an agent was in when it was attached. It republishes through `republish` alone, **not**
`publish_roster_change`: both consumers that act on a status follow `StreamSessionAgents`, and a
status ticks on every tool call, so putting a whole roster on the session room for each one would
spend the room's bandwidth on a badge.

A conversation opened for a **peer's** session records nothing — the roster naming that agent is on
the daemon facilitating it.

| Site | State |
|---|---|
| `OpenAgentConversation` | `Open` |
| `PromptAgentConversation`, before either branch | `Prompting` |
| the managed-codebase dispatch | `ExecutingTool`, then back to `Prompting` |
| a local turn resolving | `Open` (guarded) |
| a forwarded turn's relayed stream ending | `Open` (guarded) |
| `CancelAgentConversation` | `NoConversation` |
| `DetachSessionAgent` | forgotten entirely |
| `ReportAgentConversationState` | whatever the jail reported, clamped |

`relay_watching_for_the_turn_to_end` exists for one reason: a remote agent's turn loop runs on its
owning daemon, but the roster reporting its status is held here, so a forwarded turn would otherwise
raise the badge to `RUNNING` and never lower it. It passes the peer's frames and errors through
verbatim and in order — the caller must not be able to tell a relayed stream from a direct one.

### Known concurrency limitation

`persist_roster` read-modify-writes the whole `.session.yaml` under only this store's mutex, which
no other writer of that file holds — `tddy_core::update_activity_status` does the same
read-modify-write on every `ReportSessionStatus` hook. Interleaved with an attach, the roster can
revert on the next daemon restart. Recorded in [docs/dev/TODO.md](../../../docs/dev/TODO.md).

## `session_agent_clone`

Two sides, and the distinction matters when reading the code:

| Type | Runs on | Represents |
|---|---|---|
| `SessionAgentCloneStore` / `AgentClone` | the **facilitating** daemon | what A knows about a checkout it asked B to cut |
| `HostedAgentClones` / `HostedClone` | the **owning** daemon | the checkout B actually holds, and how B serves it |

### Provisioning

One clone per **(session, owning daemon)**, shared by every agent that daemon owns — two agents on
one host reading one tree is the common case, and a checkout each would multiply disk and sync cost
for isolation a read-only mirror does not need.

`claim` is reached from two callers — an attach, and a start seeding an agent this daemon does not
own — and both get the same clone under the same key. It mints the id **before** contacting the peer, reusing the split-placement discipline from
[remote-managed-worktree.md](../../../docs/ft/daemon/remote-managed-worktree.md): a forward that
times out still leaves A able to name — and therefore delete — whatever B built. `ClaimedAgentClone`
carries whether this attach *commissioned* the clone, so a second agent on the same host can never
unwind a clone the first one is still using.

The checkout is a **detached worktree under B's sessions base**, not `start_workspace_session`'s
branch workflow: that workflow fetches `origin/<branch>` (impossible for a remote-less project) and
would name a branch in the operator's own repository that the mirror moves off two seconds later.
It is still an ordinary listable, deletable `workspace` session.

### Room admission

The owning daemon does **not** self-mint its session-room token. The facilitating daemon
mints a scoped, short-TTL (5 min) admission token in `provision_agent_clone`, records the
owning daemon in a `SessionAdmissionRegistry`, and forwards the token inside
`StartSessionRequest.agent_clone` (`AgentClonePlacement.first_admission_token`). The owning
daemon joins `session-{session_id}` with that token.

`run_clone_mirror` runs a **re-admit loop**: on room disconnect it calls
`SessionAdmissionService.AdmitOwningDaemon` over the common room for a fresh token and
rejoins, preserving `CloneMirror` state across the reconnect. A `FAILED_PRECONDITION`
re-admit is the revocation signal — the facilitating daemon revokes the owning daemon on the
last detach (`tear_down_agent_clone`) and on session delete (`revoke_all_for_session`), and
the mirror stops cleanly rather than retrying a room it is no longer welcome in.

### The mirror

`run_clone_mirror` runs the [session worktree sync](../../../docs/ft/daemon/session-worktree-sync.md)
client algorithm **in-process** on B, against `session-{session_id}`. `tddy-session-sync` is a
library dependency rather than a reimplementation — a second implementation of "restore from the WIP
ref, apply a delta, notice a divergence" would be two mirrors that disagree, and the disagreement is
silent.

`restore()` only runs its local-changes check when `mirror.marker().last_seq == restored_at_seq`,
i.e. when the mirror itself wrote nothing since the last restore. Without that gate every ordinary
edit produced a false *"the clone was modified underneath the mirror"* at `error`, because the
divergence check ran before the fetch that advances the ref and therefore listed exactly the files
the delta had just legitimately written.

### The tool split

`HostedClone::execute_tool_on_facilitator` is the write half. Reads
(`READ`/`GLOB`/`GREP`/`SEMANTIC_SEARCH`/`READ_LINTS`) are served from B's own clone with no round
trip; mutations (`WRITE`/`STR_REPLACE`/`DELETE`/`SHELL`/`AWAIT`) proxy to A and land in the
**authoritative** worktree. A mutation applied to the clone would be overwritten by the next sync
tick and would never reach the session's branch.

`connection_service.rs` runs the hosted-clone branch of `execute_tool` / `stream_execute_tool`
**after** `authorize_exec_tool_caller` and before local worktree resolution. That ordering is not
cosmetic: the branch selects a worktree that is not this daemon's and proxies its mutations under
the clone's own credential, so authenticating after it would let an unauthenticated caller write to
another host's tree.

### Readiness is reported, not inferred

`ReportAgentCloneState` is pushed by B, because only the daemon holding the checkout can say whether
it is usable. A prompt sent while a clone is `PROVISIONING` is refused naming the state — queuing it
would make a 90-second `git clone` look like a hung agent, and serving it would read an empty
checkout and report "not found" for a file that is merely not there yet.

The handler authenticates `session_token` before recording anything: the
(session, daemon, codebase_session_id) triple is published in the `session.agents` broadcast, so it
identifies a clone but does not authorize a claim about it.

## The sandbox-IPC bridge

A managed-codebase session's agent runs in a jail, and its tool calls reach the
facilitating daemon over the sandbox `SessionChannel`. The roster and conversation
RPCs (`StreamSessionAgents`, `OpenAgentConversation`, `PromptAgentConversation`,
`CancelAgentConversation`) ride that same channel rather than a second transport:

- **`SessionFrame` carries two new payload variants** — `RpcRequest` (jail → host) and
  `RpcStreamFrame` (host → jail), multiplexed by `request_id` the way tunnels already are, so a
  lifetime-long roster stream does not occupy the positionally-paired request/response slot that
  `ExecuteTool` uses.
- **`tddy-sandbox-runner`** dispatches an inbound `RpcRequest` to an injected `HostRpcHandler`
  (`run_host_relay_with_rpc`); a unary reply becomes one terminal `RpcStreamFrame`, a server-stream
  reply becomes payload frames followed by a terminal marker, and a handler error becomes a
  terminal frame carrying the message. `NullRpcHandler` refuses every call with `UNIMPLEMENTED`, so
  a standalone app or test that wires no daemon stays honest.
- **`tddy-daemon`** implements `HostRpcHandler` as `DaemonRpcHandler`, which recovers the
  `Arc<ConnectionServiceImpl>` through a self-handle (`Arc<OnceLock<Weak<Self>>>` set once in
  `main.rs`) so a `&self` tonic trait method can dispatch to the roster/conversation handlers
  without the call site threading the Arc through.

The in-jail `tddy-tools` roster stream client opens `StreamSessionAgents` over this bridge; it
reconnects with backoff on drop, and a roster that never receives a frame (`RosterCurrency::Unreachable`) does not enforce tool withdrawal, while one that was current and went stale still does.

## Tests

| File | Covers |
|---|---|
| `tests/session_agent_roster_acceptance.rs` | attach/detach/list/stream, revisioning, persistence, auth, traversal refusal |
| `tests/session_agent_replacement_acceptance.rs` | what the roster withdraws, through `roster_replacement_pairs` — the function the spawn paths actually call |
| `tests/session_agent_conversation_acceptance.rs` | cancelling a turn that is still in flight; conversation frame bounding |
| `tests/session_agent_remote_acceptance.rs` | two real daemons in a LiveKit room: resolution, room membership, clone sharing, the read/write split driven through a real turn loop, teardown |
| `src/session_agent_status.rs` (unit) | the status mapping, the reporter clamp, the guarded turn end, per-pair keying, summary truncation |
| `src/session_agent_roster.rs` (unit) | `roster_entry` populating `status`/`last_activity`, and the clone outranking the conversation |

The remote suite needs Docker. Run it with `--test-threads=1`, and prefer a shared testkit
(`./run-livekit-testkit-server`, then `LIVEKIT_TESTKIT_WS_URL=…`) — per-test containers collide on
host ports and produce failures that move between runs.

## Related

- [Session agent roster](../../../docs/ft/daemon/session-agent-roster.md) — the feature
- [Session rooms](../../../docs/ft/daemon/session-room.md) — the room an owning daemon joins
- [Session worktree sync](../../../docs/ft/daemon/session-worktree-sync.md) — the mirror algorithm
- [Remote managed worktree](../../../docs/ft/daemon/remote-managed-worktree.md) — the workspace-session and tool-proxy primitives reused here
- [Connection service](connection-service.md) — the RPCs and conversation routing
