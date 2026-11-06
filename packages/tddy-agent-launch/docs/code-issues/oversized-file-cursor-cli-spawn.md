# oversized-file: cursor_cli_spawn.rs

**Location:** `packages/tddy-agent-launch/src/cursor_cli_spawn.rs`
**Category:** oversized-file
**Detected:** 2026-11-03 — `/pr-wrap` step 3.5 file-length gate on #796 (`#agent-tools` 7/12), production lines counted to the first `#[cfg(test)]`
**Metrics:** **684 production lines** (counted to the first `#[cfg(test)]`) · budget 500 · **1.37× over**
**Thresholds breached:** length 587 > 500
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — regressed 2026-11-05 by #820 (+2) — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-03 | 429 → 586 | #796 (`#agent-tools` 7/12) pushed it past the budget: `spawn_cursor_from_spec` and its `CursorSpecStart` (the host start from `cursor.yaml`), `resolve_cursor_agent`, `no_coding_agents`, the hook-token metadata write before launch, and `#[allow(dead_code)]` + `TODO(cursor-cli)` markers on `prepare_cursor_cli_launch` / `spawn_cursor_cli_process`. Not split: the developer deferred all code-restructuring decisions in `#agent-tools` to the stack's sweep node (developer, 2026-11-02) |
| 2026-11-03 | 586 → 587 | #797 (`#agent-tools` 8/12): **worse by one** — one line in `spawn_cursor_from_spec` (the shared `launch_host_pty` gained an `on_exit` argument, `None` here). Not split: deferred to the sweep node (developer, 2026-11-02) |
| 2026-11-05 | 682 → 684 | **worse by 2** in #820: the host start binds through `harness_template_params` (`HarnessLaunch` with the chat and the node launcher) instead of `cursor_template_params`; measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap (the whole-file count to the first `#[cfg(test)]`; the earlier rows used another count). Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |

## What would close it

Deleting the dead `prepare_cursor_cli_launch` and `spawn_cursor_cli_process` (~60 lines, awaiting approval: `docs/dev/todo/2026-11-03-cursor-hand-built-launch-paths.md`) and moving `spawn_cursor_from_spec` / `CursorSpecStart` (~110 lines) beside `cursor_from_spec.rs` would take it under budget. Prove the seams with `restructure check --deep` before applying.
