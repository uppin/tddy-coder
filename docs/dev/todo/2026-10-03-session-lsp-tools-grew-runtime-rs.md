# 2026-10-03 — session LSP tools grew runtime.rs, already over the 500-production-line budget

**Category:** Deferred decomposition
**Source:** `#live-plan` 11/15, [#570](https://github.com/uppin/tddy-coder/pull/570), `/pr-wrap` file-length gate.

## What is left

This PR added the selection of the index-backed `Lsp*` executor to the daemon's composition root and
deferred its decomposition with the developer's consent:

| File | Production lines |
|---|---|
| `packages/tddy-daemon/src/runtime.rs` | 1,636 → 1,650 — see its `oversized-file-runtime` code-issue record |

The 14 lines are inside `build`: the executor is now built there, its idle-reaper registry taken, and
`select_lsp_executor` registers either it or the index-backed one — a block that had to move below the
`index_daemon:` section, because that section's registry is what the selection needs.

## Why it was left

Splitting a composition root is its own refactor, and other `#live-plan` nodes touch the same file
(#571, #573), so the rename fallout would conflict through the stack.

## What would close it

`runtime.rs` at or under 500 production lines, measured with the `/pr-wrap` step 3.5 gate. The seams
are designed in the code-issue record. The executor-selection block is one contiguous run of `build`
and can move with the other subsystem wiring groups.
