# complexity: read_turn

**Location:** `packages/tddy-agent-backend/src/backend/spec_headless.rs` — `read_turn`
**Category:** complexity
**Detected:** 2026-11-05 — `/green` on #821 (`#agent-tools` 15/19), fn line to closing brace
**Metrics:** **146 lines** · nesting 4 · 5 parameters · budget 60 / 4
**Thresholds breached:** length 146 > 60
**Status:** Open — deferred to the #agent-tools sweep node

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-05 | 146 | 4 | first detection — the three copies of `run_child` (claude 288, cursor 265, codex 238 lines) became this one: the progress/output/debug callbacks, the conversation-output log, the adapter read, the wait, the exit judgement and the response |

## What would close it

Extract the sink closures into a `TurnSinks` struct built from the request (`progress`, `raw output`, `debug`, `conversation log`), the conversation-log header into `fn log_request`, and the exit judgement plus response assembly into `fn response_of(stream, status, stderr)`; `read_turn` then reads as sinks → read → wait → judge.
