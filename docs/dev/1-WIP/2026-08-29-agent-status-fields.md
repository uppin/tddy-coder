# Changeset: Session agent status and last activity

**Date**: 2026-08-29
**Status**: 🚧 In Progress
**Type**: Feature

## Affected Packages

- **tddy-service**: [README.md](../../packages/tddy-service/README.md)
  - `connection.proto` — `SessionAgentStatus` enum, `SessionAgentActivity` message, and the two
    new fields on `SessionAgentEntry` (`status` = 11, `last_activity` = 12)
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md)
  - `session_agent_status.rs` (new) — the status mapping, the per-agent activity store, and the
    tool-call/summary formatting
  - `session_agent_roster.rs` — the snapshot builder reads both live stores through
    `SnapshotSources` and populates the two new fields
  - `connection_service.rs` — records the transitions at open / prompt / tool dispatch / turn end /
    cancel / detach, and republishes the roster on each

## Related Feature Documentation

- [Session agent roster](../ft/daemon/session-agent-roster.md) — extended, not amended

## Summary

A roster entry says *what* an agent is and *whether its checkout is ready*, and nothing at all about
what it is doing. `SessionAgentEntry` gains `status` — idle / running / executing_tool /
waiting_for_input / connecting / error — and `last_activity` (a millisecond timestamp plus one short
summary line). Both are built from signals the daemon already has: the clone store decides
`CONNECTING` and `ERROR`, and the managed `AgentConversation` decides the rest.

Both fields ride the existing `StreamSessionAgents` server stream. There is no status RPC, for the
reason every roster read is already a whole snapshot: a reader that rebuilt its registry from a
snapshot and then had to correlate a second stream to learn what each row was doing is a reader that
can show a status for a row it no longer holds.

## Scope

- [x] **Proto**: the enum, the activity message, the two entry fields
- [x] **Implementation**: daemon population from the existing signals
- [x] **Testing**: unit tests for the status mapping, the activity store, the summary formatting,
      and the snapshot builder
- [ ] **Package Documentation**: `packages/tddy-daemon/docs/session-agent-roster.md` (wrap step)
- [ ] **Web**: the Agents-tab display — separate PR, `agents-tab-realtime-status`
- [ ] **MCP**: the `subagent_status` tool — separate PR, `subagent-status-tool`
- [ ] **Non-managed inference**: separate PR, `subagent-conversation-inference`

## Technical Changes

### State A (current)

`SessionAgentEntry` carries identity (`agent_id`, `name`, `daemon_instance_id`), display
(`label`, `model`), the frozen tool sets (`replaces`, `tools`) and the checkout
(`codebase_session_id`, `clone_state`, `clone_error`). `roster_entry` reads exactly one live store,
`SessionAgentCloneStore`. Nothing anywhere reports whether an agent has been asked anything.

### State B (target)

`roster_entry` reads two live stores, taken together as `SnapshotSources` so neither can be
consulted without the other — an entry carrying a fresh `clone_state` and a stale `status` is two
different accounts of one agent. The second store is `SessionAgentActivityStore`, keyed by
`(session_id, agent_id)`.

### Delta

#### tddy-service

- `SessionAgentStatus` — six values plus `UNSPECIFIED`. `UNSPECIFIED` is *"this daemon has nothing
  to say"*, never "idle": it is what a roster restored from `.session.yaml` after a restart honestly
  reports.
- `SessionAgentActivity { at_unix_ms, summary }` — one pre-truncated display line and the time
  behind it. A summary with no time reads as current forever.
- `SessionAgentEntry.status` (11), `SessionAgentEntry.last_activity` (12).

#### tddy-daemon — `session_agent_status.rs` (new)

- `ManagedAgentState` — what the daemon knows one conversation to be doing: `NoConversation`,
  `Open`, `Prompting`, `ExecutingTool`, `WaitingForInput`.
- `ManagedAgentState::from_activity_status` — the session-hook vocabulary
  (`tddy_core::session_activity::SessionActivityStatus`) mapped onto that, so a hook-shaped signal
  and a conversation-shaped one land on the same status. This is what the follow-on
  non-managed-inference PR consumes.
- `agent_status(clone_state, activity)` — the mapping under test. **The clone is read first and
  wins outright**: an agent whose checkout is still provisioning *refuses* prompts, so reporting it
  `IDLE` because no turn is in flight would offer the operator an agent that cannot answer.
  `AGENT_CLONE_STATE_UNSPECIFIED` is treated as `CONNECTING` for the same reason `roster_entry`
  refuses to call it `READY` — an unmeasured checkout is one no prompt may be served from.
- `SessionAgentActivityStore` — the per-agent map, keyed by the **pair**: one def attached to two
  sessions is one `agent_id`, and keying on it alone would show a turn on one session as a turn on
  the other. Nothing here is persisted, and that is the point: a status written to `.session.yaml`
  and read back would claim a turn is in flight in a process that never started one.
- `tool_call_summary` — a tool name plus the one argument that names what it acted on. The whole
  argument object is deliberately not carried: a `Write`'s `content` is the file, and a roster
  snapshot past `MAX_CHUNK_FRAME_BYTES` is chunk-framed, where one lost frame wedges the call with
  no error. Summaries are truncated at 120 characters on the way in for the same reason.

#### tddy-daemon — `session_agent_roster.rs`

- `SnapshotSources<'a> { clones, activity }` replaces the bare `&SessionAgentCloneStore` throughout
  the snapshot path. `SessionAgentRosterStore::new` takes the activity store and hands it back out
  through `activity()`, so a recorded turn and the snapshot that reports it are the same map.
- `roster_entry` populates `status` and `last_activity`. The last activity **survives** a state
  change rather than being cleared with it: what an idle agent was last seen doing is the only
  useful thing on its row.

#### tddy-daemon — `connection_service.rs`

- `note_agent_activity` records and republishes together. A status change does not move `rev` — the
  roster itself did not change — so a subscriber that heard about `rev` changes alone would show the
  state an agent was in when it was attached until the next attach, which may never come. This is
  the reason `republish` already exists for clone reports.
- A conversation opened for a **peer's** session records nothing: the roster naming that agent is on
  the daemon facilitating it, and a status recorded against a session this daemon only holds a clone
  for is one nothing will ever read.
- Transitions: `OpenAgentConversation` → `Open` ("conversation opened"); `PromptAgentConversation` →
  `Prompting` ("prompted: …"); the managed-codebase dispatch → `ExecutingTool` around each call and
  back to `Prompting` after it (not `Idle` — the tool returned, the turn did not);
  `CancelAgentConversation` → `NoConversation`; `DetachSessionAgent` → forgotten entirely, so a
  re-attach does not inherit the previous attachment's last activity.
- A turn that **fails** goes to `Idle`, not `ERROR`: the agent is still attached and still
  promptable, and `ERROR` is reserved for the clone. The summary is what says what happened.
- `relay_watching_for_the_turn_to_end` — a remote agent's turn loop runs on its owning daemon, but
  the roster reporting its status is held here, so a forwarded turn would otherwise raise the badge
  to `RUNNING` and never lower it. The peer's frames and errors are passed through verbatim and in
  order: the caller must not be able to tell a relayed stream from a direct one, which is the same
  property `AgentConversation`'s two variants exist to hold.

## Testing

Unit tests in `session_agent_status.rs` and `session_agent_roster.rs`:

- the clone outranks the conversation — provisioning is `CONNECTING` and a failed clone is `ERROR`
  however idle the conversation looks; an unmeasured remote clone is `CONNECTING`, not `IDLE`
- with a usable checkout the conversation decides, across all five states
- nothing observed is `UNSPECIFIED`, never `IDLE`
- the hook vocabulary maps onto agent states, both directions of the `Started`/`Done` collapse
- the store answers per agent, and the same `agent_id` on two sessions is two records
- a detach forgets; deleting a session forgets only its own agents
- an idle entry still shows what it was last seen doing
- one agent's turn does not appear on another agent's row
- summaries: multi-line prompts collapse to one line, long prompts are cut on **characters** (a cut
  mid-codepoint panics), a `Write`'s payload never reaches the summary

## Known limitations (deliberate, in scope for the follow-ons)

- **Non-managed conversations infer nothing.** An agent whose loop is not a daemon-held
  `AgentConversation` reports `UNSPECIFIED`. That is PR `subagent-conversation-inference`.
- **An agent this daemon *owns* but does not facilitate records nothing.** Its status would have to
  be pushed back to the facilitating daemon the way `ReportAgentCloneState` pushes clone state; the
  facilitating daemon's own relay covers the coarse running/idle transition in the meantime, so a
  remote agent shows `RUNNING` and `IDLE` but never `EXECUTING_TOOL`.
- **`WAITING_FOR_INPUT` has no producer yet.** The value and its mapping exist; nothing a managed
  agent does currently blocks on a human.
- **The session activity hooks are session-scoped.** `ReportAgentActivity` carries no `agent_id`, so
  it is not attributed to roster rows — attributing the main agent's hook status to every agent on
  the roster would be worse than saying nothing. `from_activity_status` is the bridge for when a
  per-agent hook signal does exist.
