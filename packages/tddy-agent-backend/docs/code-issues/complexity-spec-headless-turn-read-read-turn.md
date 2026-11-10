# complexity: read_turn

**Location:** `packages/tddy-agent-backend/src/backend/spec_headless/turn_read.rs` — `read_turn`
**Moved:** 2026-11-07 by #829 (`#agent-tools` 21/28), from `spec_headless.rs` to `spec_headless/turn_read.rs` (engine `move_item`, body unchanged)
**Category:** complexity
**Detected:** 2026-11-05 — `/green` on #821 (`#agent-tools` 15/19), fn line to closing brace
**Metrics:** **141 lines** · nesting 4 · 5 parameters · budget 60 / 4
**Thresholds breached:** length 146 > 60
**Status:** Open — deferred to the #agent-tools sweep node

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-05 | 146 | 4 | first detection — the three copies of `run_child` (claude 288, cursor 265, codex 238 lines) became this one: the progress/output/debug callbacks, the conversation-output log, the adapter read, the wait, the exit judgement and the response |
| 2026-11-05 | 141 | 4 | `/pr-wrap` on #821: the conversation log is opened by `open_conversation_log` before the spawn (out of this function); a missing stdout reaps the child. Better, not clean |
| 2026-11-07 | 141 → 141 | 4 | **unchanged, moved** in #829: `read_turn` moved with `open_conversation_log` and the stderr reader into `spec_headless/turn_read.rs`; fn line to closing brace on `origin/master` (`d3fe1abea`) and HEAD |

## What would close it

Extract the sink closures into a `TurnSinks` struct built from the request (`progress`, `raw output`, `debug`, `conversation log`), the conversation-log header into `fn log_request`, and the exit judgement plus response assembly into `fn response_of(stream, status, stderr)`; `read_turn` then reads as sinks → read → wait → judge.
