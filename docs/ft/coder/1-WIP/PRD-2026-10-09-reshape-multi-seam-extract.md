# Multi-seam plans: every operation sees the files the plan has already written - PRD

**Date**: 2026-10-09
**PRD Type**: Enhancement (fixes a check/apply parity defect)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `### Moved code that changes meaning one module deeper` (a seam cut after another one), `## What a run executes` (what `check --deep` vouches for, and the limitation `check --deep` does not compile), the `check` row of the command table (it now prints widenings).

No other feature document changes. No new operation, plan field, flag or wire message.

## Summary

A plan that cuts several `extract_module` seams out of one file resolves each seam against the tree the plan has produced so far, including the files earlier seams created. A reference from an earlier seam's file to an item a later seam moves keeps that item widened, is imported where the later seam needs it, or is refused as stranded when no facade is asked for. `check --deep` and `apply --dry-run` answer the same as `apply`, and `check --deep` prints every widening the apply would report.

## Background

`#carve` 3 cut eight seams out of `crate_move.rs` in one plan. `check --deep` reported no findings and `apply` reported 8 of 8; the tree then failed to compile with 13 errors needing 6 hand fixes, all imports or visibility. `#carve` 13 hit the same shape in nine seams (`use super::strip_resize;` unresolved). Every remaining split of an oversized file — `#reshape` 15 and 17 — is a multi-seam plan.

The engine already counts a reference in another file as "reached from outside". What it lacks is a server that knows those files exist. Each operation opens only its anchor file. In `check --deep` and `apply --dry-run`, files created by earlier operations live only in memory, so rust-analyzer is never shown them. In `apply` they are on disk, and the engine depends on rust-analyzer's own file watcher, which the index daemon has recorded as blind on this repository's workspace within a request. So the survey misses the sibling's references, the import pass treats the sibling's names as already broken, and the visibility pass narrows back what the sibling needed. The narrowing is reported nowhere.

## Proposed Changes

### What's Changing

- **The plan's projected tree is shown to the server.** Before an operation of a run asks rust-analyzer anything, the backend opens every file earlier operations of the same run created or changed, with the text the run has for it: in memory for `check --deep` and `apply --dry-run`, on disk for `apply`. A file an earlier operation renamed is shown under its new path only. They are closed when the operation ends, like every document an operation opens. This applies to every operation the Rust backend resolves through the server, not only `extract_module`.
- **Visibility.** An item a later seam moves stays widened, and is reported as widened, when an earlier seam's file reaches it. A glob facade then re-exports it.
- **Imports.** A name in a later seam that is defined in, or reached through, an earlier seam's module is imported like any other lost name, with the import verified as today.
- **Stranded references.** With `reexport: none`, a reference in an earlier seam's file to an item this seam moves is refused by name, as a reference in any other file is today. The engine does not rewrite references in files other than the anchor's.
- **`check --deep` prints widenings.** Each widening `apply` would report is printed by `check --deep` in the same `visibility:` form, beside the notes it already prints.
- **Parity.** For a plan of several seams, `check --deep`, `apply --dry-run` and `apply` reach the same refusals and the same widenings.

### What's Staying the Same

- `extract_module`'s own rules: which items move, facade shapes, rebased relative visibility, re-rooted inline paths, refusals.
- One operation per server question; no re-indexing between operations, no new waits.
- `check --deep` still does not compile the projected tree; a clean deep check can still be followed by a compile-gate failure for reasons outside this change (backlog items I, K, M, W of the 2026-09-24 record).
- `Workspace`, the apply loops in the runner and the index daemon, and the plan format are unchanged.
- Widening is still to `pub(crate)`; choosing a narrower `pub(super)` is not part of this change.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring` only: a small per-run record of written files on `RustBackend` (beside the existing record of module names a plan claimed), opening them before an operation's first server request, and `Rehearsed` carrying the operation's widenings to the deep check's account. New logic goes in new functions; no function over 60 lines grows.
- One new live rust-analyzer test binary, registered in `.config/rust-e2e.filterset` and the `rust-analyzer` test group in `.config/nextest.toml`.
- Cost: one `didOpen`/`didClose` pair per file written so far, per operation. A ten-seam plan opens at most twenty extra documents on its last operation.

### User Impact

- A multi-seam plan that `check --deep` passes no longer lands the visibility and import errors above. A plan that would strand a reference is refused at `check --deep` instead of after an apply has rewritten several files.
- `check --deep` output gains `visibility:` lines for plans that widen. No other output changes. No breaking change.

## Implementation Plan

1. Confirm rust-analyzer resolves a module declared in an open document whose file exists only as another open document (the rehearsal case), with one live test.
2. The per-run record of written files, folded from each resolution the backend returns, with unit tests for create, change and rename.
3. Open the record's files before an operation's first server request; close them with the rest.
4. `Rehearsed` carries the widenings; `check --deep` prints them.
5. Live acceptance binary: the stranded refusal, the kept widening (gap J), the import, and the three-way parity; registered in the filterset and test group. (The existing seam suites missing from the group are registered by `#reshape` 1, not here.)
6. Docs at wrap: feature doc, `packages/tddy-code-restructuring/docs/assist-output-repairs.md` and `readiness-and-gates.md`.

## Acceptance Criteria

- [ ] a two-seam plan whose first seam's file calls an item the second seam moves behind a glob facade applies, compiles, and reports the item widened ([Rust code restructuring](../rust-code-restructuring.md))
- [ ] the same plan under `apply --dry-run` and `check --deep` reports the same widening
- [ ] with `reexport: none`, `check --deep` refuses that second seam, naming the item and the earlier seam's file, and `apply` refuses it the same way, writing nothing
- [ ] a later seam's code that calls a helper an earlier seam moved gets an import of it, in a dry run and in an apply
- [ ] a three-seam plan of the `#carve` 3 shape (references across all three seams) passes `check --deep` with no findings, applies 3 of 3 and compiles with its tests
- [ ] single-seam plans and the existing seam suites (`sibling_module_paths_acceptance`, `inline_paths_acceptance`, `import_pass_acceptance`) are unchanged
- [ ] tests pass for `tddy-code-restructuring` (scoped; CI for the rest)

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-multi-seam-extract.md`
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-multi-seam-extract-initial-discovery.md`
- Todo this closes: [extract_module cannot see sibling seams in one plan](../../../dev/todo/2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan.md)
- Todo this narrows (removes J): [restructure apply gaps from the lifecycle destructure run](../../../dev/todo/2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md)
- Why the server's watcher cannot be relied on: `packages/tddy-index-daemon/src/tree_changes.rs`
- Pass design: `packages/tddy-code-restructuring/docs/assist-output-repairs.md`, `packages/tddy-code-restructuring/docs/readiness-and-gates.md`
