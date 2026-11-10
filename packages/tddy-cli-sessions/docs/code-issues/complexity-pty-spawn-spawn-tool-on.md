# complexity: spawn_tool_on

**Location:** `packages/tddy-cli-sessions/src/cli_session_manager/pty_spawn.rs:75` — `CliSessionManager::spawn_tool_on`
**Category:** complexity
**Detected:** 2026-11-10, `#agent-tools` sweep (#876) structural re-measurement
**Metrics:** **75 lines** (2026-11-10) · nesting depth 3 · 4 branch/match lines · 12 parameters · 3 `?` exits
**Thresholds breached:** length 75 > 60 (`/analyze-clean-code`); depth and complexity within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 sweep); unclaimed

Deferred by the user, 2026-11-10 (#876 sweep): the restructure engine refused the seam (missing imports for builder types / early returns in the range); revisit after the engine fix

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-10 | 104 → 75 | 3 | #876 B4i/Tier C: terminal registration, ready await, spawn-spec build, cleanup-and-finish extracted. Every remaining seam (recording hook, task spawn, pending-exit match, handle build) names the exit hook's `Output`, which this file does not import. |

## What would close it

Bring it under 60 lines by `extract_method` once the engine spells fully-qualified associated
type names. Anchor with `tddy-tools restructure anchors`, never by hand, then prove the seam
with `restructure check --deep` against a warm index.
