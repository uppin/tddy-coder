# complexity: invoke

**Location:** `packages/tddy-agent-backend/src/backend/spec_headless.rs` — `SpecHeadlessBackend::invoke`
**Category:** complexity
**Detected:** 2026-11-05 — `/green` on #821 (`#agent-tools` 15/19), fn line to closing brace
**Metrics:** **103 lines** (and `turn_for`: 67) · nesting 3 · budget 60 / 4
**Thresholds breached:** length 88 > 60; `turn_for` 67 > 60
**Status:** Open — deferred to the #agent-tools sweep node

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-05 | 88 / 67 | 3 | first detection — `invoke`: system-prompt file choice, render, turn files and pointer, spawn, the deadline `select`; `turn_for`: the protocol applied to the request |
| 2026-11-05 | 103 / 67 | 3 | **regressed** at `/pr-wrap` on #821: a turn with no working directory is refused (no process-cwd fallback), and the conversation log is opened before the spawn so a log that cannot be opened leaves no child to reap |

## What would close it

`invoke`: extract the system-prompt-file choice into `fn system_prompt_delivery` and the session-pointer assembly into `fn pointer_for(request, workdir)`. `turn_for`: split the scratch directory and the session choice into two small functions.
