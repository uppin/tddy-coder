# 2026-10-04 — session restructure tools grew runtime.rs and tddy-tool-engine's lib.rs, both over the 500-production-line budget

**Category:** Deferred decomposition
**Source:** `#live-plan` 14/15, [#573](https://github.com/uppin/tddy-coder/pull/573), `/pr-wrap` file-length gate.

## What is left

This PR registered the restructure executor and routed the `restructure_*` tool names, and deferred
both decompositions with the developer's consent:

| File | Production lines |
|---|---|
| `packages/tddy-daemon/src/runtime.rs` | 1,668 → 1,677 — see its `oversized-file-runtime` code-issue record |
| `packages/tddy-tool-engine/src/lib.rs` | 869 → 873 — see its `oversized-file-lib` code-issue record |

The 9 lines in `runtime.rs` are inside `build`: the `IndexRestructureExecutor` registration beside the
`Lsp*` one, from the same `IndexChannel`. The 4 lines in `lib.rs` are the `mod restructure_tools;`
declaration and the one dispatch arm that hands the six names to it; the handler itself is in the new
`restructure_tools.rs`.

## Why it was left

`runtime.rs`: splitting a composition root is its own refactor, and the stack's parents touch the same
file, so the rename fallout would conflict through the stack. `lib.rs`: no stack node touches it, but
bringing an 873-line file under budget takes several engine seams — a node of its own, not a side
effect of a four-line change.

## What would close it

Each file at or under 500 production lines, measured with the `/pr-wrap` step 3.5 gate. The
`runtime.rs` seams are designed in its code-issue record; `lib.rs`'s are `extract_module --to_file`
moves, per its record.
