# complexity: start_reaper_and_users_watch

**Location:** `packages/tddy-supervisor/src/lib.rs:133` — `start_reaper_and_users_watch`
**Category:** complexity
**Detected:** 2026-11-10, `#agent-tools` sweep (#876) wrap re-measurement (brace matching, fn line to closing brace; nesting read by hand as control-flow depth)
**Metrics:** **61 lines** (2026-11-10) · nesting depth 2
**Thresholds breached:** length 61 > 60 (`/analyze-clean-code`); depth within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 wrap); unclaimed

Deferred by the user, 2026-11-10 (#876 wrap): left for a later refactoring node

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-10 | 61 | 2 | new in #876: extracted from `run` (B4a). It holds the reaper task and the users.yaml watch, with the watch's `SupervisorUsersSink` declared inside it. The `/pr-wrap` gate's awk and an earlier scan both stopped at this file's `#[cfg(test)] mod test_util;` declaration on line 57, so neither measured it |

## What would close it

Move the `SupervisorUsersSink` declaration and its impl out of the function body, or extract the
users-watch start into its own function. Anchor with `tddy-tools restructure anchors`, never by hand,
then prove the seam with `restructure check --deep` against a warm index.
