# 2026-09-26 — Turn control, an addressable transcript, and a subagent that says when it read nothing

**Type:** Feature + Bug Fix

`SubagentSession::take_turn(TurnRequest)` replaces `prompt`, and both implementors take it. Two new
modules carry what it needs: `subagent::turn_request` (a new question, a continuation, a rewind, a
correction — and a per-call budget bounded to `1..=50`, clamped to the nearer bound and reported)
and `subagent::transcript` (ids that are never reused after a rewind, previews cut to 240
characters, boundary-snapping that never separates a tool call from its results).

A budget spent entirely on tool calls of which **none ran** is an `Err` returned **before any model
call** — the synthesis turn asks for "the specific file:line locations you found" with nothing that
checks anything was found, and at `temperature: 0.0` a model obliges deterministically. The
conversation is left as the failed call built it and stays promptable. The guard sits on the
budget-exhaustion path alone.

`dispatch_tool_call` returns a typed outcome with `serde_json`-built payloads instead of a
`format!`-interpolated string, so a tool error containing a quote or a newline is still valid JSON.
`CodebaseAccess::read_window` applies the 200-line cap on **both** paths — resolved into the
request on Managed, applied to bytes in hand on Local. `TurnEnd::took_a_turn`'s premise is
corrected. `RemoteAgentSession` gains real resume and id support; it refuses one shape the local
session accepts (a turn that is both a new prompt and a rewind), for which no MCP caller exists.

[roster-and-subagent-runtime.md](../roster-and-subagent-runtime.md) · [../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md](../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
