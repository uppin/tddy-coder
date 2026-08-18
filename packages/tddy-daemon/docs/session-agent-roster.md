# Session agent roster modules (`tddy_daemon::session_agent_roster`, `::session_agent_clone`)

## Role

Two modules implement a session's **agent roster** — the set of specialized agents attached to a
live session, addressable as `name@daemon_instance_id`, mutable while the session runs. See
[docs/ft/daemon/session-agent-roster.md](../../../docs/ft/daemon/session-agent-roster.md) for the
feature and its acceptance criteria.

| Module | Owns |
|---|---|
| `session_agent_roster` | The roster itself: membership, `rev`, persistence, subscriptions. |
| `session_agent_clone` | Everything about a **remote** agent: the checkout on its owning daemon, the mirror that keeps it current, and the hosted-side tool split. |

The RPCs, the conversation routing and the room wiring live in `connection_service.rs` — there is
deliberately no `session_agent_conversation.rs`, because routing a conversation needs the peer
plumbing, the room handle and the roster in one place.

## `session_agent_roster::SessionAgentRosterStore`

```rust
pub fn new(clones: Arc<SessionAgentCloneStore>) -> Self
pub fn snapshot(&self, session_id, session_dir)      -> Result<SessionAgentRoster, Status>
pub fn attach(&self, session_id, session_dir, record) -> Result<SessionAgentRoster, Status>
pub fn detach(&self, session_id, session_dir, agent_id) -> Result<SessionAgentRoster, Status>
pub fn entry(&self, session_id, session_dir, agent_id) -> Result<SessionAgentEntry, Status>
pub fn agents_owned_by(&self, session_id, session_dir, daemon) -> Result<Vec<..>, Status>
pub fn subscribe(&self, session_id, session_dir)     -> Result<(snapshot, Receiver), Status>
pub fn republish(&self, session_id, session_dir)     -> Result<(), Status>
```

It holds the clone store because a roster **entry** is a projection of two things: the persisted
record, and the live clone state that only the clone store knows. `roster_entry` reads both, which
is why an entry's `clone_state` is current without the roster having to be rewritten every time a
clone advances.

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

`commit()` **persists then adopts**: a failed write leaves the in-memory store agreeing with the
file it will be rebuilt from, so a restart never silently undoes an attach the operator was told
succeeded. Persistence goes through `tddy_core::write_session_metadata` →
`atomic_file::write_atomic_labelled`.

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

`claim` mints the id **before** contacting the peer, reusing the split-placement discipline from
[remote-managed-worktree.md](../../../docs/ft/daemon/remote-managed-worktree.md): a forward that
times out still leaves A able to name — and therefore delete — whatever B built. `ClaimedAgentClone`
carries whether this attach *commissioned* the clone, so a second agent on the same host can never
unwind a clone the first one is still using.

The checkout is a **detached worktree under B's sessions base**, not `start_workspace_session`'s
branch workflow: that workflow fetches `origin/<branch>` (impossible for a remote-less project) and
would name a branch in the operator's own repository that the mirror moves off two seconds later.
It is still an ordinary listable, deletable `workspace` session.

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

## Tests

| File | Covers |
|---|---|
| `tests/session_agent_roster_acceptance.rs` | attach/detach/list/stream, revisioning, persistence, auth, traversal refusal |
| `tests/session_agent_replacement_acceptance.rs` | what the roster withdraws, through `roster_replacement_pairs` — the function the spawn paths actually call |
| `tests/session_agent_conversation_acceptance.rs` | cancelling a turn that is still in flight; conversation frame bounding |
| `tests/session_agent_remote_acceptance.rs` | two real daemons in a LiveKit room: resolution, room membership, clone sharing, the read/write split driven through a real turn loop, teardown |

The remote suite needs Docker. Run it with `--test-threads=1`, and prefer a shared testkit
(`./run-livekit-testkit-server`, then `LIVEKIT_TESTKIT_WS_URL=…`) — per-test containers collide on
host ports and produce failures that move between runs.

## Related

- [Session agent roster](../../../docs/ft/daemon/session-agent-roster.md) — the feature
- [Session rooms](../../../docs/ft/daemon/session-room.md) — the room an owning daemon joins
- [Session worktree sync](../../../docs/ft/daemon/session-worktree-sync.md) — the mirror algorithm
- [Remote managed worktree](../../../docs/ft/daemon/remote-managed-worktree.md) — the workspace-session and tool-proxy primitives reused here
- [Connection service](connection-service.md) — the RPCs and conversation routing
