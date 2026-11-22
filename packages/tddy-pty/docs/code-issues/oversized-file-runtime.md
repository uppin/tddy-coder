# oversized-file: runtime.rs

**Location:** `packages/tddy-pty/src/runtime.rs`
**Category:** oversized-file
**Detected:** 2026-11-10 — `/pr-wrap` step 3.5 file-length gate on #876 (`#agent-tools` 33/33 sweep), production lines counted to the first `#[cfg(test)]`
**Metrics:** **529 production lines** (2026-11-22; 533 on 2026-11-19, 523 on 2026-11-10) · budget 500
**Thresholds breached:** length 529 > 500
**Restructure:** `move_item` / `move_impl_members` into a submodule — `/code-restructuring` territory
**Status:** Open — partially fixed 2026-11-22 (533 → 529, still over); deferred by the user, 2026-11-10 (#876 wrap); unclaimed

Deferred by the user, 2026-11-10 (#876 wrap): left for a later refactoring node

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-11-10 | 451 → 523 | #876: the Tier C extracts out of `open_pty_and_pump` (179/9 → 40) added each helper's signature and struct (`spawn_child_and_pump`, `PtyMasterSetup`, `spawn_reader_with_master`, `wait_for_reader_drain`, `spawn_pty_writer`, `spawn_pty_reader`), pushing the file past the budget |
| 2026-11-15 | 523 → 523 | #905 touched the exit monitor (one line: the status now comes from `exit_status::task_status_of`) and added its tests below `#[cfg(test)]`; production lines unchanged |
| 2026-11-19 | 523 → 533 | remote-agent pid-per-session fix: `PtySpawnSpec.exit_code_in_output` (field + doc, 3 lines), its import (2) and the read after the exit monitor (5). The read itself and its tail bound live in `exit_status.rs` (`ExitCodeInOutput::read_from_capture`) to keep this file's growth to the call site |

## What would close it

Move the PTY I/O helpers (`PtyMasterSetup`, `spawn_reader_with_master`, `wait_for_reader_drain`, `spawn_pty_writer`, `spawn_pty_reader`, `PTY_READ_BUF`, `READER_DRAIN_GRACE`, about 125 lines) into `runtime::pty_io` with one `move_item` carrying `name`. Anchor with `tddy-tools restructure anchors`, never by hand, prove the seam with
`restructure check --deep` against a warm index (`./run-index-daemon`), and confirm the file and the
new module are each ≤ 500 production lines.
| 2026-11-22 | 533 → 529 | #997: the terminal variables a PTY child is told (`TERMINAL_VARIABLES`, now shared with a target agent's `env -i`) moved with `set_terminal_variables` to `terminal_variables.rs`, so the new shared constant grew this file by nothing |
