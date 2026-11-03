# oversized-file: svc_start_sandboxed_cursor_cli_session.rs

**Location:** `packages/tddy-agent-launch/src/svc_start_sandboxed_cursor_cli_session.rs`
**Category:** oversized-file
**Detected:** 2026-11-03 — `/pr-wrap` step 3.5 file-length gate on #796 (`#agent-tools` 7/12), production lines counted to the first `#[cfg(test)]`
**Metrics:** **549 production lines** · budget 500 · **1.1× over**
**Thresholds breached:** length 549 > 500
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-03 | 467 → 549 | #796 (`#agent-tools` 7/12) pushed it past the budget: `CursorJail`, `launch_jailed_cursor` and `write_jail_hooks` (the jailed start through `launch_jailed`), replacing the hand-built runner argv and bridge (which shrank `start_sandboxed_cursor_cli_session` ~431 → ~363 lines). Not split: the developer deferred all code-restructuring decisions in `#agent-tools` to the stack's sweep node (developer, 2026-11-02) |

## What would close it

Move `CursorJail`, `launch_jailed_cursor` and `write_jail_hooks` (~120 lines) into their own module beside `claude_jail.rs` — the move `docs/dev/todo/2026-11-03-cursor-hand-built-launch-paths.md` recommends instead of deleting the file.
