# oversized-file: mem.rs — the in-memory `Fs` adapter

**Location:** `packages/tddy-fs/src/mem.rs`
**Category:** oversized-file
**Detected:** 2026-11-26 by the `/pr-wrap` file-length gate on #1047 (`#fs-port` 3/13)
**Metrics:** **702 production lines** (`origin/master` 483 → HEAD 702, +219: the additive port
surface — `hard_link`, `create_dir_all_with_mode`, `remove_dir`, `metadata_follow`,
`FsMetadata::{uid, dev, ino}`, `FsKind::Socket` — implemented for `MemFs` plus its unit tests
in-file) · budget 500 · ~1.4× over
**Thresholds breached:** length 699 > 500
**Restructure:** required — module seams (the per-operation `impl` blocks or the node tree vs
the `Fs` implementation)
**Status:** Open — grown past the budget by this PR; split deferred: every other `#fs-port`
stack node touches `packages/tddy-fs` (verified 2026-11-26: all of `activity-fs`,
`agents-fs`, `code-tools-fs`, `hosts-fs`, `lifecycle-fs`, `rest-fs`, `sandbox-fs`,
`session-files-fs`, `supervisor-fs`, `workflow-fs` diff `tddy-fs` against n1's tip), so a
rename here cascades through the whole stack. The sweep node after the stack lands splits it.

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-26 | 483 → 702 | #1047 (`#fs-port` 3/13): the additive port extension lands on the `MemFs` adapter with dual-adapter conformance tests. Stack-shared — deferred, not split here |

## What would close it

Take the file under 500 with the `code-restructuring` skill after the `#fs-port` stack lands.
Candidate seams (re-derive with `restructure anchors` against the tree of the day):

| Seam | Items |
|---|---|
| A | the in-memory node tree (`Node`, path normalization, `paths`) |
| B | the additive operations (`hard_link`, `create_dir_all_with_mode`, `remove_dir`, `metadata_follow`) with their unit tests |
