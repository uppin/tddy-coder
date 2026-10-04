# 2026-10-04 — `cursor_cli_spawn.rs` crossed the 500-line file budget in `#live-plan` 12/15

**Category:** Deferred from `#live-plan` 12/15 (#571)
**Source:** `/pr-wrap` step 3.5 file-length gate on #571, production lines counted to the first `#[cfg(test)]` (the file has none, so the whole file)

## What grew it

`packages/tddy-session-lifecycle/src/cursor_cli_spawn.rs` went from **445** to **532** lines at base `c3567fde`, crossing the 500 budget in this PR (+87). The growth is the `spawn_cursor_cli_session_reporting` wrapper and the phase reporting around `spawn_cursor_cli_session_inner`. Nothing else in the stack touches the file.

Re-measure: `awk '/#\[cfg\(test\)\]/{exit} {n++} END{print n}' packages/tddy-session-lifecycle/src/cursor_cli_spawn.rs`

## Why decomposition was deferred

The developer consented, at `/pr-wrap` on #571 (2026-10-04), to defer decomposing this file. The file's size comes from `spawn_cursor_cli_session_inner`, a single large function, and the `code-restructuring` engine refuses its remaining seams (`SpawnStackParent<'_>` and the early returns). Splitting the file is therefore the extract-method work the existing record already describes, not a module move.

## What would close it

Extract the reporting wrapper and its phase helpers into a sibling module once the function's own seams are movable, so the file drops back under 500. This is a `/code-restructuring` job, scheduled together with the open code issue rather than inside a feature stack.

Tracked in `packages/tddy-session-lifecycle/docs/code-issues/complexity-cursor-cli-spawn-spawn-cursor-cli-session-inner.md`, which carries the measurement history, is set to regressed and is unclaimed.
