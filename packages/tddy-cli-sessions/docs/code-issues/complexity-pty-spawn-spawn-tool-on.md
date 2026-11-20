# complexity: spawn_tool_on

**Location:** `packages/tddy-cli-sessions/src/cli_session_manager/pty_spawn.rs:109` — `CliSessionManager::spawn_tool_on`
**Category:** complexity
**Detected:** 2026-11-10, `#agent-tools` sweep (#876) structural re-measurement
**Metrics:** **77 lines** (2026-11-19; 75 on 2026-11-10) · nesting depth 3 · 4 branch/match lines · 13 parameters · 3 `?` exits
**Thresholds breached:** length 75 > 60 (`/analyze-clean-code`); depth and complexity within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 sweep); unclaimed

Deferred by the user, 2026-11-10 (#876 sweep): the restructure engine refused the seam (missing imports for builder types / early returns in the range); revisit after the engine fix

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-10 | 104 → 75 | 3 | #876 B4i/Tier C: terminal registration, ready await, spawn-spec build, cleanup-and-finish extracted. Every remaining seam (recording hook, task spawn, pending-exit match, handle build) names the exit hook's `Output`, which this file does not import. |
| 2026-11-17 | 75 → 75 | 3 | #929 (`#activity-wake` 1/3): re-measured — `spawn_tool_on` untouched; the agent-end announcement landed in `spawn_terminal_cleanup` and `agent_end.rs`. Unchanged, stays open |
| 2026-11-19 | 75 → 77 | 3 | remote-agent pid-per-session fix: the `exit_code_in_output` parameter (12 → 13 parameters) and its one-line assignment onto the built spec. Worse, stays open |
| 2026-11-20 | 77 → 77 | 3 | #959 (`#session-liveness` 2/4): re-measured — `spawn_tool_on` untouched (no parameter added); the tmux-client filter landed in `spawn_cleanup_for`, the per-terminal exit handling in `spawn_terminal_cleanup` (59 lines). Unchanged, stays open |

## What would close it

Bring it under 60 lines by `extract_method` once the engine spells fully-qualified associated
type names. Anchor with `tddy-tools restructure anchors`, never by hand, then prove the seam
with `restructure check --deep` against a warm index.
