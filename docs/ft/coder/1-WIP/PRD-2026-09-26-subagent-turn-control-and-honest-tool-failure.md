# Subagent Turn Control and Honest Tool Failure - PRD

**Date**: 2026-09-26
**PRD Type**: Bug Fix + Enhancement

## Affected Features

- **Primary**: [Managed-Codebase Mode + Discovery Subagents](../managed-codebase-subagents.md) — the
  `subagent_*` MCP contract gains a per-call turn budget, a `subagent_resume` tool, and message ids
  on every turn outcome; a prompt whose every tool call failed becomes an error instead of an answer
- **[Discovery Agent (FastContext)](../discovery-agent.md)** — the agent whose answers this PRD
  stops fabricating; no change to the agent itself, but its failure mode changes shape
- **[Session Agent Roster](../../daemon/session-agent-roster.md)** — a conversation to an agent on
  the facilitating daemon is the *only* kind in practice, so the roster's conversation RPCs carry
  the new budget, resume and message-id fields
- **[Sandboxed Codebase Mode](../../daemon/sandboxed-codebase-mode.md)** — a jailed tool call can no
  longer wedge its session's IPC channel, and a jail whose channel dies is rebuilt once
- **[Remote Codebase Mode](../../daemon/remote-codebase-mode.md)** — the proxied `Read` tool honours
  the `offset`/`limit` it has always advertised

## Summary

A specialized subagent can spend its entire turn budget on tool calls that all failed and still
answer with a confident, file:line-cited summary of nothing. It happened on 2026-09-26: FastContext
made 54 tool calls, every one refused, and reported a line number, a constant value, a working
directory and a tool list it had invented. Nothing in the system said the tools were dead — not to
the agent, not to the main agent reading its answer.

This PRD makes three changes to that chain, in one delivery because they are the same chain:

1. **A tool failure that is the channel's stops looking like a tool failure that is the command's.**
   Both the jail boundary and the subagent's own dispatch gain a typed transport-failure signal. A
   jail whose channel dies is torn down and rebuilt once, rather than staying dead for the life of
   the session.
2. **A subagent that read nothing says so.** A prompt in which no tool call succeeded returns an
   error naming the failure, and never reaches the synthesis turn that asks a model to cite
   locations it never saw.
3. **The main agent gets control of the subagent's turn budget.** `subagent_prompt` accepts an
   optional `maxTurns`; a new `subagent_resume` continues a conversation with a fresh budget —
   optionally rewound to a named earlier message and corrected — so a chain of tool calls cut short
   by the budget can be finished rather than restarted.

And two defects on the same path that this change would otherwise step around: a jailed shell
inherits the runner's IPC stdin (the root cause of the incident), and the `Read` tool ignores the
`offset`/`limit` it advertises.

## Background

The incident, verified from `~/.tddy/sessions/*/tool-calls.jsonl`, `~/.tddy/logs/daemon` and the
still-live process tree. Full evidence in the changeset's
[initial discovery](../../../dev/1-WIP/2026-09-26-subagent-turn-control-and-honest-tool-failure-initial-discovery.md).

A `Shell` tool call ran `grep -n -A30 "interface ITheme" $f` with `$f` empty. `grep` fell back to
reading **stdin**, which — because `tokio::process::Command::output()` leaves stdin inherited,
unlike its `std` counterpart — was the `tddy-sandbox-runner --stdio` tool-IPC request pipe. The
Shell tool's 30-second timeout dropped the future without killing anything, so the orphan stayed a
rival reader on the daemon→jail request channel.

The next tool call blocked. Ten minutes later the 600-second in-jail deadline fired and the channel
was marked permanently closed, which is the documented and correct policy for a channel that lost
its answer. From then on every tool call in that session was refused.

Twelve minutes after that, FastContext was consulted. It spent all ten of its turns on tool calls
that were all refused, and was then handed the synthesis prompt: *"Summarize your findings now from
what you have already read, citing the specific file:line locations you found."* There is no check
that anything was read. Asked again on the same conversation, it produced byte-identical output —
`temperature` is `0.0`, so the fabrication was deterministic, not a glitch.

Three separate design decisions each made sense alone and compounded:

- A dead channel and a failing command are reported identically, *deliberately*, so the caller
  cannot tell them apart by shape.
- A spent turn budget lands softly in a summary, so the caller gets what was gathered rather than
  nothing.
- A broken jail stays broken, because the alternative is running the session's tools on the host it
  was jailed away from.

Together they turn a dead socket into a citation.

## Proposed Changes

### What's Changing

#### Per affected feature: [managed-codebase-subagents.md](../managed-codebase-subagents.md)

**`subagent_prompt` gains `maxTurns`.** Optional. Absent, the agent definition's `max_turns` applies
exactly as today. Present, it overrides for that call only and is clamped to an absolute ceiling of
**50**. A value above the ceiling is clamped rather than refused, and the outcome says the budget
was clamped so a caller is never silently given less than it asked for.

**New tool `subagent_resume`.**

```json
{"type":"object","required":["sessionId"],
 "properties":{
   "sessionId":{"type":"string"},
   "fromMessageId":{"type":"string","description":"Rewind the conversation to just after this message, discarding everything later. Omitted: continue from the end."},
   "correction":{"type":"string","description":"A single corrective instruction appended after the rewind point. Omitted: continue with no new input."},
   "maxTurns":{"type":"integer","minimum":1},
   "graceMs":{"type":"integer","minimum":0}}}
```

It sends **no new prompt turn**. With neither `fromMessageId` nor `correction` it continues the
existing history with a fresh budget — the case where a main agent judges the agent was mid-chain
when its budget ran out. With them it goes back in time and corrects.

The `correction` field exists because the provider is called at `temperature: 0.0`. A rewind with no
new input re-sends identical context and reproduces the same turn; the incident demonstrated exactly
that, twice, to the byte.

**Every turn outcome enumerates the messages it appended.** `subagent_prompt`, `subagent_await` and
`subagent_resume` return, alongside `{stopReason, content, usage}`:

```json
"messages":[
  {"id":"m18","role":"assistant","toolCalls":["Read"]},
  {"id":"m19","role":"tool","tool":"Read","isError":true,"preview":"…its channel is closed"}
]
```

Ids are stable and monotonic within a conversation and are **never reused**; a rewind permanently
retires the ids it discards. `preview` is truncated — a single `Read` result in the incident was
42 KB. `isError` is the field whose absence let the incident run: it is what a main agent reads to
see that a subagent's work is failing rather than progressing.

**A prompt whose every tool call failed is an error.** No synthesis turn, no model call, so nothing
can be invented. The error names the transport failure. The conversation stays open and resumable —
its history is intact and `subagent_resume` can rewind into it.

#### Per affected feature: [sandboxed-codebase-mode.md](../../daemon/sandboxed-codebase-mode.md)

**A jailed shell no longer inherits the runner's stdin,** and a Shell timeout kills the command's
process group rather than abandoning it.

**A dead jail channel is rebuilt once.** The `WorkspaceSandbox` boundary gains a typed
transport-failure outcome, distinct from a tool that ran and failed. On that outcome and only that
outcome, the jail is torn down, re-provisioned, and the tool call retried exactly once. A second
failure is reported to the caller; the retry is not a loop.

#### Per affected feature: [remote-codebase-mode.md](../../daemon/remote-codebase-mode.md)

**`Read` honours `offset` and `limit`.** It has advertised both since the catalog was written and
implemented neither. A subagent forwards them correctly and the daemon discards them, so a jailed
agent's 32k context fills with whole files — the opposite of what the 200-line cap was for.

### What's Staying the Same

- **A broken channel is still never failed over to the host.** The jail is rebuilt or the call
  fails. Running a jailed session's tools on the host worktree remains refused, in every path.
- **A roster entry still carries no endpoint, credential or agent definition.** `maxTurns` is passed
  by the caller per call and bounded by the ceiling; editing a definition still cannot change what a
  running session may do. The ceiling is what keeps `server.rs:1755-1759`'s guarantee true in
  substance — a caller may ask for a longer search, not for an unbounded one.
- **The soft landings stay soft.** `MaxTurnRequests` and `ContextExhausted` keep today's behaviour
  exactly. Only the all-tools-failed case, which has no landing today, becomes an error.
- **A conversation still runs one turn at a time.** `subagent_resume` queues like any other turn and
  reports `queuePosition` / `queueSize` the same way.
- **The 600-second in-jail deadline is unchanged.** With the wedge prevented and a relaunch in
  place, shortening it would trade one failure mode for another; it is a two-sided constant shared
  by the runner and the daemon and is left alone.
- **`max_turns` does not become a `models.db` column.** A per-call override makes one unnecessary.

## Impact Analysis

### Technical Impact

**Every subagent conversation in this deployment is remote.** `subagent_new_session` runs an agent's
loop in-process only when that process holds the definition; the jail holds none. So `maxTurns`,
resume, rewind and message ids are **proto changes**, not merely Rust ones —
`PromptAgentConversationRequest` has no budget field and `AgentConversationChunk` carries only
`content_chunk`, `stop_reason`, `last`. A new resume operation must also be added to the in-jail
relay allowlist or the jail cannot call it.

`SubagentSession` is a trait with two implementors, one local and one relayed. A per-call budget and
a resume change the trait and therefore both — and `RemoteAgentSession` today returns empty history
and zero usage, with standing TODOs saying so.

The advertised MCP surface is pinned by name and count in an audit test; a seventh `subagent_*` tool
moves it. The sandboxed-Claude allowlist is a separate hand-maintained list that must gain the new
tool or it is advertised and uncallable.

The retry has one viable home — the single layer that holds the sandbox registry and sits beneath
all three dispatch entries, including the roster agent's own turn loop. It needs one new constructor
field, and its constructor has one call site.

Two packages in the change's path, `tddy-tool-engine` and `tddy-discovery`, have no
`docs/code-issues/` directory: they have not been analyzed, which is not the same as clean.

### User Impact

- **A main agent stops being lied to.** The observable change is that a subagent consulted during a
  tool outage returns an error naming the outage, instead of a plausible wrong answer. The incident
  cost a session's worth of work and very nearly cost a code change made against invented facts.
- **A subagent's exploration can be finished rather than restarted.** Today a chain cut short by the
  budget can only be re-asked, re-reading everything. `subagent_resume` continues it.
- **A wrong turn can be undone.** A main agent that sees `isError` on a tool result, or a search
  going the wrong way, can rewind to that message and correct it, instead of abandoning the
  conversation.
- **A session survives a jail death.** Previously terminal; now a rebuild and one retry.
- **No breaking change.** Every new input field is optional; every new output field is additive.

## Implementation Plan

1. **Typed transport failure, both surfaces.** The `WorkspaceSandbox` boundary and the subagent's
   tool dispatch each learn to report "the channel failed" distinctly from "the tool failed". Both
   later milestones depend on this and neither can be written before it.
2. **Stop the wedge.** `stdin` closed on every spawned shell; Shell timeout kills the process group.
3. **Rebuild a dead jail.** Tear down, re-provision, retry once, at the one layer that can.
4. **Honest failure.** No synthesis turn when nothing succeeded; error carries the transport failure;
   the conversation stays resumable.
5. **`Read` honours `offset`/`limit`** in the engine, closing a contract tested on its sending side
   only.
6. **Turn budget over the wire.** `maxTurns` on the MCP tool and on the conversation RPC, bounded.
7. **Message ids and resume.** Ids minted on append, enumerated in every turn outcome, carried over
   the conversation RPC; `subagent_resume` with rewind and correction; the advertisement audit and
   the sandboxed-Claude allowlist updated together.

Testing approach per area is in the changeset. The constraint that shapes it: the real-jail
acceptance suite runs on no CI machine (macOS-gated, Linux-only CI), so the gate is the ungated
host-relay suites plus new unit and integration coverage, with a Seatbelt proof added but not relied
on.

## Acceptance Criteria

- [x] AC1 — A shell command spawned for a jailed session cannot read the runner's IPC channel
      ([sandboxed-codebase-mode.md](../../daemon/sandboxed-codebase-mode.md))
- [x] AC2 — A Shell call that exceeds its budget leaves no surviving descendant process
- [x] AC3 — A transport failure at the jail boundary is reported distinctly from a tool that ran and
      failed, and a non-zero exit from a command is **not** a transport failure
- [x] AC4 — A jail whose channel dies is torn down, re-provisioned and the call retried exactly
      once; a second failure is reported and does not relaunch again
- [x] AC5 — A jailed session's tools are never run on the host worktree, on any path, including
      after a failed relaunch
- [x] AC6 — A prompt in which no tool call succeeded returns an error naming the transport failure
      and makes no synthesis model call ([managed-codebase-subagents.md](../managed-codebase-subagents.md))
- [x] AC7 — A conversation whose prompt failed that way is still open, and `subagent_resume` can
      rewind into its history
- [x] AC8 — A prompt in which *some* tool calls succeeded keeps today's `MaxTurnRequests` soft
      landing
- [x] AC9 — The daemon's `Read` returns only the requested line window and reports whether more
      lines follow ([remote-codebase-mode.md](../../daemon/remote-codebase-mode.md))
- [x] AC10 — `subagent_prompt` accepts `maxTurns`, honours it for that call only, and clamps it to
      the ceiling of 50, reporting that it clamped
- [x] AC11 — `subagent_prompt` without `maxTurns` uses the agent definition's budget, unchanged
- [x] AC12 — Every turn outcome enumerates the messages it appended, with id, role, tool name,
      `isError` and a truncated preview
- [x] AC13 — Message ids are stable, monotonic and never reused; ids discarded by a rewind are never
      minted again
- [x] AC14 — `subagent_resume` with no arguments beyond `sessionId` continues the conversation with a
      fresh budget and sends no new prompt turn
- [x] AC15 — `subagent_resume` with `fromMessageId` discards every message after it, and the rewound
      history is what the next turn is sent
- [x] AC16 — `subagent_resume` with `correction` appends exactly one corrective message after the
      rewind point
- [x] AC17 — `subagent_resume` on an unknown `sessionId` or `fromMessageId` is an error, never a
      silent continue
- [x] AC18 — A rewind never leaves a tool-call message without its matching tool results
- [~] AC19 — `maxTurns`, resume and message ids work for a conversation whose loop runs on the
      facilitating daemon ([session-agent-roster.md](../../daemon/session-agent-roster.md)) —
      **implemented, not proven end to end**: the suite that would drive it across the wire runs on
      no CI machine, so this rests on compile-checking plus either side of the gap
- [x] AC20 — `subagent_resume` is advertised in the MCP catalog and present in the sandboxed-Claude
      allowlist
- [x] AC21 — Documentation updated: the `subagent_*` tool table, the stop-reason list (which is
      already missing `context_exhausted`), and the sandboxed-codebase jail lifecycle

## References

### Affected Features (Complete List)

- [managed-codebase-subagents.md](../managed-codebase-subagents.md) — MCP tool contract: `maxTurns`,
  `subagent_resume`, message ids, all-tools-failed error
- [discovery-agent.md](../discovery-agent.md) — FastContext's failure mode
- [session-agent-roster.md](../../daemon/session-agent-roster.md) — conversation RPCs carry the new
  fields
- [sandboxed-codebase-mode.md](../../daemon/sandboxed-codebase-mode.md) — jail stdin isolation,
  channel death and relaunch
- [remote-codebase-mode.md](../../daemon/remote-codebase-mode.md) — `Read` windowing

### Related Documentation

- [Initial discovery](../../../dev/1-WIP/2026-09-26-subagent-turn-control-and-honest-tool-failure-initial-discovery.md)
  — the incident forensics and the three exploration passes
- [`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`](../../../dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md)
  — records the missing crash detector and restart policy this PRD partly closes
- [`docs/dev/todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md`](../../../dev/todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md)
  — why the real-jail suite cannot be the gate

### Code issues filed by this PRD's planning pass

Four oversized-file records, all in files this change edits. Recorded rather than restructured, by
the developer's decision, so this stays one reviewable PR:

- `packages/tddy-tools/docs/code-issues/oversized-file-server.md` — 2,488 production lines, holds
  the whole `subagent_*` MCP surface
- `packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md` — 1,208 lines and **no
  in-file tests at all**
- `packages/tddy-discovery/docs/code-issues/oversized-file-subagent-runtime.md` — 557 lines; also
  records the false `took_a_turn` premise
- `packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md` — 793 lines; also records the
  three unguarded shell spawn sites and the ignored `Read` window

`tddy-discovery`, `tddy-tool-engine` and `tddy-daemon-sandbox` have **never** had the CRAP pipeline
run against them; these records measure file length only, and "not measured" is not "clean".
