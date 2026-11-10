# complexity: invoke

**Location:** `packages/tddy-agent-backend/src/backend/spec_headless.rs` — `SpecHeadlessBackend::invoke`
**Category:** complexity
**Detected:** 2026-11-05 — `/green` on #821 (`#agent-tools` 15/19), fn line to closing brace
**Metrics:** **91 lines** (and `turn_for`: 67) · nesting 3 · budget 60 / 4
**Thresholds breached:** length 88 > 60; `turn_for` 67 > 60
**Status:** Open — partially fixed 2026-11-07 by #829 (104 → 91); `invoke` and `turn_for` still over 60

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-05 | 88 / 67 | 3 | first detection — `invoke`: system-prompt file choice, render, turn files and pointer, spawn, the deadline `select`; `turn_for`: the protocol applied to the request |
| 2026-11-05 | 103 / 67 | 3 | **regressed** at `/pr-wrap` on #821: a turn with no working directory is refused (no process-cwd fallback), and the conversation log is opened before the spawn so a log that cannot be opened leaves no child to reap |
| 2026-11-06 | 103 → 104 | 3 | **worse by 1** in #823 (`#agent-tools` 17/19): the pointer it writes carries `socket: Some(..)` and an empty `notify` (`SessionPointer` gained both); fn line to closing brace on `origin/feature/agent-tools/spec-backend` (`d325fba73`) and HEAD. Restructuring is the sweep's (n20) |
| 2026-11-07 | 104 → 91 | 3 | **better by 13** in #829 (`#agent-tools` 21/28): the turn's session pointer is built by `turn_pointer` (it also gained `owner`); fn line to closing brace on `origin/master` (`d3fe1abea`) and HEAD. Still over 60, and `turn_for` (67) unchanged |

## What would close it

`invoke`: extract the system-prompt-file choice into `fn system_prompt_delivery` and the session-pointer assembly into `fn pointer_for(request, workdir)`. `turn_for`: split the scratch directory and the session choice into two small functions.
