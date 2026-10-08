# 2026-10-08 — `cursor_cli_spawn.rs` in `tddy-agent-launch` is 563 production lines, over the 500 budget

**Category:** Deferred from `lifecycle-moves` (#536): oversized file, developer-consented deferral
**Source:** #carve 21/21 (PR #536), step 3.5 of `/pr-wrap` and acceptance check B4 of
[`2026-09-26-carve-lifecycle-moves`](../1-WIP/2026-09-26-carve-lifecycle-moves.md)

`packages/tddy-agent-launch/src/cursor_cli_spawn.rs` is **563 production lines** by the changeset's counter
(562 by `/pr-wrap`'s count-to-the-first-`#[cfg(test)]` script), against the 500-line budget. It is the only file
this node moved into a new crate at or over 500. The receivers' other large files are inherited (`session_room.rs`, `config.rs`, …).

## What the PR did to it

It moved the file **whole**, with `move_cluster_to_crate`, from `tddy-session-lifecycle/src/cursor_cli_spawn.rs`.
Before the move it was 548 production lines by `/pr-wrap`'s count (549 by the changeset's), so it was already over the
budget and this PR did not push it across. The growth of 14 lines is paths and formatting: `use` lines and body paths
re-spelled through their defining crate (`crate::connection_service::X` became `tddy_session_split::service_util::X`
and the like), `rustfmt` wrapping the signatures and calls that got longer, and `mod chat; mod resume;` becoming
`pub use crate::{chat, resume};`. A diff of the old and the new file shows no statement added, removed or reordered
(36 lines in, 22 out, all of those kinds).

## Why it is deferred

This PR is a mechanical move, and the node's boundary is "no deduplication, no functional refactor". Splitting a
file is a restructure of its own. The developer **consented to deferring the split** in this PR (2026-10-08).

## What would close it

The file measures under 500 production lines. No plan is recorded here: how to cut it has not been decided.
`spawn_cursor_cli_session_reporting` alone is 267 lines. A complexity record for the file's entry point,
`complexity-cursor-cli-spawn-spawn-cursor-cli-session-inner.md`, sits in `packages/tddy-session-lifecycle/docs/code-issues/` and moves with the code.
`restructure check --budget 500` takes the budget directly, and the counter above re-measures it.
