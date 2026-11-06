# oversized-file: cursor_cli_spawn.rs

**Location:** `packages/tddy-agent-launch/src/cursor_cli_spawn.rs`
**Category:** oversized-file
**Detected:** 2026-11-03 — `/pr-wrap` step 3.5 file-length gate on #796 (`#agent-tools` 7/12), production lines counted to the first `#[cfg(test)]`
**Metrics:** **587 production lines** · budget 500 · **1.2× over**
**Thresholds breached:** length 587 > 500
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — regressed 2026-11-03 by #797 (+1) — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-03 | 429 → 586 | #796 (`#agent-tools` 7/12) pushed it past the budget: `spawn_cursor_from_spec` and its `CursorSpecStart` (the host start from `cursor.yaml`), `resolve_cursor_agent`, `no_coding_agents`, the hook-token metadata write before launch, and `#[allow(dead_code)]` + `TODO(cursor-cli)` markers on `prepare_cursor_cli_launch` / `spawn_cursor_cli_process`. Not split: the developer deferred all code-restructuring decisions in `#agent-tools` to the stack's sweep node (developer, 2026-11-02) |
| 2026-11-03 | 586 → 587 | #797 (`#agent-tools` 8/12): **worse by one** — one line in `spawn_cursor_from_spec` (the shared `launch_host_pty` gained an `on_exit` argument, `None` here). Not split: deferred to the sweep node (developer, 2026-11-02) |

## What would close it

Deleting the dead `prepare_cursor_cli_launch` and `spawn_cursor_cli_process` (~60 lines, awaiting approval: `docs/dev/todo/2026-11-03-cursor-hand-built-launch-paths.md`) and moving `spawn_cursor_from_spec` / `CursorSpecStart` (~110 lines) beside `cursor_from_spec.rs` would take it under budget. Prove the seams with `restructure check --deep` before applying.
