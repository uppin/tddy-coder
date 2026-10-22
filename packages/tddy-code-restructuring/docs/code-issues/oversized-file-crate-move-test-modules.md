# oversized-file: crate_move/test_modules.rs — classification and follow edits in one module

**Location:** `packages/tddy-code-restructuring/src/crate_move/test_modules.rs`
**Category:** oversized-file
**Detected:** 2026-10-21 by the `/pr-wrap` file-length gate on #611
**Metrics:** **664 production lines** (0 before this PR) · budget 500
**Restructure:** required — `extract_module --to_file` × 2, `/code-restructuring` territory
**Status:** Open — overrun accepted by the developer 2026-10-21; owner is the `#reshape` stack's sweep node

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-21 | 664 | first detection. The file is new in #611 (`#reshape` 13/24 `tests-follow`), so this PR created the overrun. No other node of the stack edits it, so the split is not blocked by stack overlap |
| 2026-10-22 | 664 | unchanged by #612 (`#reshape` 17/24), which did not touch the file; measured by `tddy-tools restructure lines`. Listed as a deferred split, seams not yet decided, in `docs/dev/todo/2026-10-21-reshape-oversized-files-splits-deferred.md` |

## What the gate found

The file holds three ideas, and the first two are separable by contiguous runs of items.

| Seam | Items | Module |
|---|---|---|
| Classification: which sibling test modules follow | `sorted`, `declared_file`, `placed_by_attribute`, `spanned_files`, `read_paths`, `Outside`, `inside_the_module`, `reaching`, `spelled_at` | `classification` |
| Follow edits: what taking a follower along writes | `follow_changes`, `followed`, `landing_collision` | `follow` |
| Types, outside-reference filtering, the travelling set, the test gate, the static check | what is left | stays |

## What would close it

Two `extract_module` operations with `to_file: true` and `reexport: named`, one per seam above, run
through `/code-restructuring` (anchors from `restructure anchors --items`, `check --deep`, then apply).
Both runs of items are contiguous. The scoped suites that prove it: `crate_move_test_modules`,
`check_precondition_parity`, `cluster_move`, `crate_move_children`, `move_module_to_crate_acceptance`.

## Verified by hand

The developer saw the split proposed at #611's wrap and accepted the overrun for now on 2026-10-21;
the split was not run.
