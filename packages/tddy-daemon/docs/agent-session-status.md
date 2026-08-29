# Agent session status inference (`tddy_daemon::session_agent_inference`)

## Role

One module infers what a **claude-cli or cursor session's agent** is doing, from what that session
already writes, and populates `SessionEntry.agent_status` / `SessionEntry.last_activity` on
`ListSessions`. See
[docs/ft/daemon/agent-session-status.md](../../../docs/ft/daemon/agent-session-status.md) for the
feature and its acceptance criteria.

It is **not** the agent roster. A roster agent is a model loop the daemon serves inside one session
([session-agent-roster.md](session-agent-roster.md)); this is the agent that *is* the session. The
two share the status vocabulary (`SessionAgentStatus`, `SessionAgentActivity`,
`ManagedAgentState`) and nothing else, so one badge renders both.

## Public API

```rust
pub fn activity_from_frame(frame: &AcpAgentMessage) -> Option<AgentActivity>
pub fn activity_from_record(record: &AgentActivityRecord) -> AgentActivity
pub fn inferred_activity(Option<SessionActivityStatus>, Option<&AgentActivity>) -> Option<AgentActivity>
pub fn session_agent_status(activity: Option<&AgentActivity>) -> SessionAgentStatus

impl SessionAgentInferenceStore {
    pub fn new() -> Self
    pub fn observe(&self, session_id, activity)
    pub fn latest(&self, session_id) -> Option<AgentActivity>
    pub fn seed_from_transcript(&self, session_id, session_dir) -> io::Result<()>
    pub fn forget(&self, session_id)
    pub fn ensure_tailing(self: &Arc<Self>, hub: &Arc<AgentActivityHub>, session_id, session_dir)
}
```

## Where the signals come from

Nothing here is a new source. A session already writes its conversation twice over, and the daemon
already broadcasts each activity record as it is recorded:

| Source | Read by | Carries |
|---|---|---|
| `acp-transcript.jsonl` + `agent-activity.jsonl`, resolved by `acp_replay::read_session_transcript` | `seed_from_transcript`, once per session | everything written before this daemon started tailing |
| `AgentActivityHub`, the per-session broadcast | the consumer task `ensure_tailing` spawns | everything recorded after |
| `.session.yaml` `activity_status`, the hook word | `list_sessions`, off the entry it just enriched | the state, where hooks are wired |

## Three properties this module exists to hold

**One mapper.** `activity_from_record` maps a live record **through**
`acp_replay::frame_for_agent_activity` and into `activity_from_frame` — the same function a replayed
frame goes through. A second mapper would let a live row and its replay word the same call
differently, and the disagreement would surface as a session that rewords its own status when the
daemon restarts and re-reads the file.

**Subscribe before seeding.** `ensure_tailing` claims the session, subscribes, spawns the consumer,
and only then reads the transcript. `seed_from_transcript` writes only when nothing has been
observed, so a record that lands during the file read is already the newer fact and is not
overwritten by what was on disk before it. Reversing the two would lose exactly that record.

**Nothing is persisted**, for the reason [`session_agent_status`](session-agent-roster.md) already
gives: a status read back from disk claims a tool call is in flight in a process that never started
one. A restarted daemon reports `UNSPECIFIED` until it has re-read a transcript.

## The rules

`inferred_activity` applies five, in order:

1. A hook word of `Done`/`Ended` **wins outright**. A `running` row whose terminal record never
   arrived would otherwise pin the badge at `EXECUTING_TOOL` for the rest of the session's life.
2. Otherwise a tool call in flight outranks the hook word — `EXECUTING_TOOL` is strictly more
   precise than `Running`, and it is the only source of the call's *name*.
3. Otherwise the hook word decides, keeping the observed summary: what an agent was last seen doing
   is the useful thing on its row.
4. With no hook word the observed signal decides alone — the cursor case, and the claude-cli case
   before the first hook fires.
5. Nothing observed and no hook word is no activity, never an idle one.

A state that came from the hook word alone carries an empty summary, so `AgentActivity::to_proto`
yields `None` and the entry sends no `last_activity` — a bare timestamp renders as an agent that did
something unnameable just now.

## Wiring

`ListSessions` (`connection_service.rs`) holds the store beside the hub it subscribes to. Inside the
existing `spawn_blocking_with_timeout` closure — the seed is a real file read, and that closure
exists to keep disk work off the reactor — each listed session whose `session_type` is `claude-cli`
or `cursor-cli` is tailed, then its two fields are populated. The gate is the one
`report_session_status` already applies: those are the session types that run an agent and write a
conversation. A `workspace` session would spend a subscription and a file read to conclude
`UNSPECIFIED`.

`DeleteSession` calls `forget`, which drops the signal and the tailing mark; the consumer task
re-checks the mark on its next record and exits rather than holding a subscription for the daemon's
life.

## Known limits

- **A seed that fails to read is not retried** — the session is claimed before the read, so a
  transient IO error leaves it seeded from nothing until a live record arrives. Un-claiming would
  have the retry spawn a second consumer beside the first.
- **The seed is read once per daemon lifetime per session.** A session whose transcript grows while
  nothing is published to the hub keeps reporting the seed; the live path is the hub by construction,
  and a re-read per listing would be a file read per session per poll.
- **No cross-daemon inference.** A session listed from a peer daemon carries whatever that daemon
  inferred.
- **`WAITING_FOR_INPUT` comes only from the hook word.** Nothing in a transcript is read as blocking
  on a human, so a cursor session with no hooks wired never reports it.
