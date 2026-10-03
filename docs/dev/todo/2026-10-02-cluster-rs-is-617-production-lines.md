# 2026-10-02 — `crate_move/cluster.rs` is 617 production lines

**Category:** Deferred from `move-paths` (#540, `#live-plan` 3/7)
**Source:** the `/pr-wrap` file-length gate on #540.

`packages/tddy-code-restructuring/src/crate_move/cluster.rs` was 611 production lines before #540 and
is 617 after it (budget 500). The six lines are call-site rewiring for the survey-driven header,
refusal and manifest passes; nothing in the file was added as new behaviour.

## Why it was deferred

The developer consented to deferral on #540. `move-facades` (#541, the next node) reworks
move-time structure in this area and its implementation is not written yet, so a decomposition here
would be rewritten under it and turn every dependent into a conflict. Do it as a follow-up branch
once the stack has landed, with `/code-restructuring` (`restructure check --budget 500`).

## What would close it

`cluster.rs` under 500 production lines, split along seams the engine proves with
`restructure check --deep`. `test_binary.rs` (966, unchanged by #540) has its own record,
`packages/tddy-code-restructuring/docs/code-issues/oversized-file-test-binary.md`.

## Also deferred: `source_scan.rs` (new in #540)

`crate_move/source_scan.rs` is 527 production lines (457 when first written; the validation refactor
split `sightings` into `Scan` methods and named constants, which cost lines). It is under the same
consent and the same reason. Natural seams: the token scanner, `use`-tree expansion, and the
module-items listing. It could also absorb the line-based scanner in `test_binary.rs`.
