# complexity: invoke

**Location:** `packages/tddy-agent-backend/src/backend/spec_headless.rs` — `SpecHeadlessBackend::invoke`
**Category:** complexity
**Detected:** 2026-11-05 — `/green` on #821 (`#agent-tools` 15/19), fn line to closing brace
**Metrics:** **88 lines** (and `turn_for`: 67) · nesting 3 · budget 60 / 4
**Thresholds breached:** length 88 > 60; `turn_for` 67 > 60
**Status:** Open — deferred to the #agent-tools sweep node

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-05 | 88 / 67 | 3 | first detection — `invoke`: system-prompt file choice, render, turn files and pointer, spawn, the deadline `select`; `turn_for`: the protocol applied to the request |

## What would close it

`invoke`: extract the system-prompt-file choice into `fn system_prompt_delivery` and the session-pointer assembly into `fn pointer_for(request, workdir)`. `turn_for`: split the scratch directory and the session choice into two small functions.
