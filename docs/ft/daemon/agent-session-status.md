# Agent session status — what a claude-cli / cursor session is doing

**Status:** ✅ Implemented (daemon); web display separate
**Product area:** Daemon (spans `tddy-service`, `tddy-daemon`)
**Date:** 2026-08-29

## Summary

A `SessionEntry` says what a session *is* — its agent, its model, its branch, whether a process is
alive — and almost nothing about what it is **doing**. The one signal that exists,
`activity_status`, is a bare hook word (`Running`, `ExecutingTool`, …) with no subject: it says the
agent is inside *a* tool call, never which one, and it is produced only by the per-worktree hooks the
daemon writes into a claude-cli or cursor worktree.

That is enough for a session the operator is already watching and not enough for a **peer agent
session** — a claude-cli or cursor session spawned as a child of an orchestrator
(`Changeset.orchestrator_session_id`), listed beside its siblings in the web's *Session agents*
section. An operator scanning five peer rows wants to see which one is stuck on a `Bash`, which one
is waiting on a permission prompt, and which one has finished — without opening five terminals.

This adds that, from stores the daemon already keeps:

- **`SessionEntry.agent_status`** — the same `SessionAgentStatus` vocabulary the agent roster reports
  (`docs/ft/daemon/session-agent-roster.md` § What an agent is doing), so one badge renders a roster
  agent and a peer session alike.
- **`SessionEntry.last_activity`** — the same `SessionAgentActivity`: one already-truncated summary
  line and the millisecond stamp behind it.

Neither is a new *source*. Both are **inferred** by tailing what the session already writes: its
persisted ACP transcript (`acp-transcript.jsonl`) and its durable agent-activity log
(`agent-activity.jsonl`) — resolved together by
`tddy_service::acp_replay::read_session_transcript`, the same view `StreamAcpReplay` replays — plus
the live records the daemon's `AgentActivityHub` broadcasts as they are recorded.

```
  claude-cli / cursor peer session
  ┌──────────────────────────────────────┐
  │ agent-activity.jsonl  (durable)      │──┐
  │ acp-transcript.jsonl  (durable)      │──┤  read_session_transcript  → seed, once
  └──────────────────────────────────────┘  │
                │ live records               │
                ▼                            ▼
        AgentActivityHub ─────────► inference store ──► SessionEntry.agent_status
        (per-session broadcast)      (newest signal)     SessionEntry.last_activity
                                            ▲
   .session.yaml activity_status ───────────┘  (the hook word, where hooks are wired)
```

## User Story

As a developer running an orchestrator with five peer agent sessions, I want each peer's row to say
what it is doing right now — `Read src/main.rs`, `Bash cargo test`, waiting for input, or finished —
so that I can tell a working agent from a stuck one without attaching to its terminal.

## Terminology

This document uses **agent session** for a session whose `session_type` is `claude-cli` or
`cursor-cli` — a session that runs a coding agent in a worktree. It is deliberately *not* a **roster
agent** (`docs/ft/daemon/session-agent-roster.md`), which is a model loop the daemon serves inside
one session; the two share the status vocabulary and nothing else.

A **peer agent session** is an agent session whose `Changeset.orchestrator_session_id` names another
session. Peers are the motivating case, but the status is populated for every agent session, because
a session is either observable or it is not — and which sessions an operator groups under an
orchestrator is a question the reader answers, not the daemon.

## What the fields carry

```proto
message SessionEntry {
  // …
  string codebase_session_id = 30;
  // What this session's agent is doing, inferred from its own transcript and activity stream.
  // `agent_status`, not `status`: field 3 is already the session's own lifecycle string.
  // UNSPECIFIED is "this daemon has nothing to say", never "idle" — the same rule the roster's
  // status follows, and for the same reason.
  SessionAgentStatus agent_status = 31;
  // The last thing it was observed doing; unset when nothing has been observed.
  SessionAgentActivity last_activity = 32;
}
```

Both enums and `SessionAgentActivity` are reused verbatim from the roster; nothing new is minted.
`activity_status` (15) stays exactly as it is — it is the raw hook word, a *reported* fact, and this
feature is an inference built partly on top of it. Removing it would break the TUI parity it exists
for.

### How a signal becomes a status

One **observed signal** per session: the newest thing seen, as a state plus a summary line plus the
stamp it was recorded at.

| The session wrote | Observed signal |
|---|---|
| a `tool_call` frame still `PENDING` / `IN_PROGRESS` | `ExecutingTool`, summary = the call's enriched title (`Read main.rs L10-49`) |
| a `tool_call` frame `COMPLETED` / `FAILED` | `Prompting`, summary = the same title — the tool returned, the turn did not |
| an `agent_message_chunk` | `Prompting`, summary = the text |
| an agent-activity record | whichever of the three rows above it maps to, through the *same* frame builder `StreamAcpReplay` uses |

A record and a frame therefore cannot disagree: an activity record is mapped to its ACP frame first
(`acp_replay::frame_for_agent_activity`) and read by the one mapper, so a live row and its replayed
counterpart produce the same summary character for character.

The **status** is then decided by the observed signal and the session's hook word together:

1. **A hook word of `Done` or `Ended` wins outright** — both are `IDLE`, through the roster's own
   bridge. A session whose agent has stopped cannot still be inside a tool call, and a `running` row
   that never got its terminal record would otherwise pin the badge at `EXECUTING_TOOL` for ever.
2. **Otherwise a tool call in flight wins** — `EXECUTING_TOOL`. It is strictly more precise than the
   hook's `Running`, and it is the only place the *name* of the call comes from.
3. **Otherwise the hook word decides**, mapped through the bridge the roster already owns
   (`ManagedAgentState::from_activity_status`).
4. **With no hook word at all, the observed signal decides alone.** This is the cursor case and the
   claude-cli case before the first hook fires.
5. **Nothing observed and no hook word is `UNSPECIFIED`** — never `IDLE`. "Attached and ready" is a
   claim, and a daemon that has seen nothing has no grounds for it.

`last_activity` is set only when there is a summary to show. A state that came from the hook word
alone leaves it unset rather than sending a bare timestamp, which a reader renders as an agent that
did something unnameable just now.

### What is observed, and when

The transcript is read **once** per session, to seed the newest signal. Everything after that arrives
live on `AgentActivityHub`, the same per-session broadcast `StreamSessionActivity` and
`StreamAcpReplay` relay. A live record always outranks the seed: the seed is only recorded when
nothing has been observed, so a record that lands while the file is being read is not overwritten by
what was on disk before it.

The subscription is taken **before** the file is read, for that same ordering.

**Nothing is persisted.** A status is a fact about a running agent; written to `.session.yaml` and
read back it would claim a tool call is in flight in a process that never started one. A restarted
daemon reports `UNSPECIFIED` until it has re-read a transcript, exactly as the roster does.

**Only agent sessions are tailed.** A `workspace` session holds a clone and runs no agent; a `tool`
or changeset session is not an agent CLI. Tailing them would spend a subscription and a file read per
listing to conclude `UNSPECIFIED`, which is what they report anyway. The gate is the same one
`ReportSessionStatus` already applies: `session_type` is `claude-cli` or `cursor-cli`.

**Summaries are truncated by the daemon**, at the same 120 characters and to a single line, by the
same `SessionAgentActivityStore` rule — a transcript title is agent-authored text of unbounded
length, and `ListSessions` is a response an operator's dashboard polls.

## Acceptance Criteria

1. A claude-cli session whose newest agent-activity row is a `running` tool call reports
   `EXECUTING_TOOL`, with that call's enriched title as `last_activity.summary`.
2. `last_activity.at_unix_ms` is the stamp the observed signal itself carries, not the time the
   listing was built.
3. A session whose newest tool call has completed and whose hooks recorded `Done` reports `IDLE`,
   and still shows that call as `last_activity` — what an idle agent was last seen doing is the
   useful thing on its row.
4. A session whose hooks recorded `Done` reports `IDLE` even when a tool call was left in flight
   with no terminal record.
5. A cursor session with no hook word at all takes its state from its ACP transcript alone.
6. A session with neither a transcript nor a hook word reports `UNSPECIFIED`, and carries no
   `last_activity`.
7. A `workspace` session reports `UNSPECIFIED` even with a transcript on disk — it runs no agent.
8. One session's tool call never appears on another session's row.
9. A record published to the hub after the session is being tailed becomes that session's reported
   activity without the transcript being re-read.
10. A summary longer than 120 characters is cut at 120, and a multi-line one is collapsed to one
    line, before it reaches the wire.

## Non-Goals

- **No new RPC and no new stream.** The fields ride `ListSessions`, which every consumer of a peer
  row already reads. A second stream to correlate against a list is the failure mode the roster's
  whole-snapshot rule exists to avoid.
- **No cross-daemon inference.** A session listed from a peer daemon carries whatever that daemon
  inferred; this daemon does not reach for another host's transcript.
- **No change to `activity_status`.** The hook word stays a reported fact with its own field.
- **`WAITING_FOR_INPUT` gets no new producer here.** The hook word already produces it
  (`Notification` → permission prompt / elicitation), and that mapping is reused; nothing in a
  transcript is read as blocking on a human.
- **The web display is a separate PR.** This one ends at the wire.

## Related

- [Session agent roster](session-agent-roster.md) — owns `SessionAgentStatus`,
  `SessionAgentActivity` and the status rules this reuses
- [Claude CLI session](claude-cli-session.md), [Cursor CLI session](cursor-cli-session.md) — the two
  session types tailed
