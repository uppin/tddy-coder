# oversized-file: svc_spawn_split_agent.rs

**Location:** `packages/tddy-session-split/src/svc_spawn_split_agent.rs`
**Category:** oversized-file
**Detected:** 2026-11-10 — `/pr-wrap` step 3.5 file-length gate on #876 (`#agent-tools` 33/33 sweep), production lines counted to the first `#[cfg(test)]`
**Metrics:** **497 → 575 production lines** (2026-11-10) · budget 500
**Thresholds breached:** length 575 > 500
**Restructure:** `move_item` / `move_impl_members` into a submodule — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 wrap); unclaimed

Deferred by the user, 2026-11-10 (#876 wrap): left for a later refactoring node

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-11-10 | 497 → 575 | #876: the Tier C extracts out of `spawn_split_agent_process` (132 → 59) added `write_split_hooks`, `place_pointer_and_spawn`, `render_and_write_split_hooks`, `render_split_argv`, `record_split_answer_wait` and `split_argv_with_flags` |

## What would close it

Move the launch-file helpers (`record_split_answer_wait`, `split_argv_with_flags` by `move_item` with `name`, then `write_split_hooks`, `place_pointer_and_spawn`, `render_and_write_split_hooks`, `render_split_argv` by `move_impl_members`, about 160 lines) into a submodule of `svc_spawn_split_agent`. Anchor with `tddy-tools restructure anchors`, never by hand, prove the seam with
`restructure check --deep` against a warm index (`./run-index-daemon`), and confirm the file and the
new module are each ≤ 500 production lines.
