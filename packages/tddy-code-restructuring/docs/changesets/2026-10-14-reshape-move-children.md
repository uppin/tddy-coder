# 2026-10-14 — cross-crate moves carry a module's directory children and keep its `mod` visibility

**Type:** Feature (engine capability for `move_module_to_crate` and `move_cluster_to_crate`) plus defect fixes; no new operation, plan field, flag or wire message

`#reshape` 5/19 (PR [#602](https://github.com/uppin/tddy-coder/pull/602), `feature/reshape/move-children`).
Product entry:
[2026-10-14-reshape-move-children.md](../../../../docs/ft/coder/changelog/2026-10-14-reshape-move-children.md).
How it works: [path-survey.md](../path-survey.md#carried-files), [facades.md](../facades.md),
[readiness-and-gates.md](../readiness-and-gates.md#the-uncarriable-child-findings-and-the-restricted-child).

## What changed

- **`crate_move/module_files.rs`.** `MovedFile { from, to, below }` and `relocated`, the one rule for where
  each file of a module lands under a new directory. `backends/rust/module_reparent/relocation.rs` `plan`
  delegates to it.
- **`crate_move/carried.rs`** (new). `fold_carried_members` (an `also` member whose module path lies under
  another member's is carried by it), `uncarriable` (the static findings: a `mod x;` leading to no file, a
  `#[path]` placement, a target already in the destination, a child another operation moves, a child's body
  reaching a module that stays behind), `carried_notes`, `restricted_children_reached` (the resolve-time
  refusal), and `TheSet`/`SetTotals`, the set the members are read against.
- **`crate_move/moving.rs`.** `Move::carried` (the module's own file first) and `Move::declaration`;
  `Move::declared_nowhere` is the one "declares no `mod`" refusal.
- **`crate_move/cluster.rs`, `cluster/gathering.rs`.** `member_changes` runs per member over its carried files:
  one rename and one re-pointed header per file, the caller survey, the refusals. `cluster/stranded.rs`
  reads every carried file's header.
- **`crate_move/header.rs`.** `repointed_header_at` takes the module path below the moved module; the former
  `repointed_header` is gone.
- **`crate_move/manifest_edits.rs`.** `ModuleDeclaration`, `declared_module` and `split_visibility`: a `mod`
  line of any visibility is a declaration. `module_declaration` and `declared_module_name` read through it.
- **`crate_move/moving/facade_writer.rs`, `crate_move.rs`.** The glob facade has the declaration's visibility;
  `FacadeEntry` and `facade_lines_for_plan` group per (destination, visibility); `written_facade` matches the
  visibility it is asked for; `leaving` is built from `glob_entries` and `extended_earlier_facades`. No file
  arriving in the destination gets a facade naming the destination.
- **`crate_move/preconditions.rs`.** `move_preconditions` runs the static findings and reads every carried
  file's body paths.
- **`crate_move/source_scan.rs`.** `Scan::continues_a_path` treats an identifier behind `..` or `..=` as the
  start of a path.
- **`crate_move.rs`.** `resolution` and `cluster_resolution` return a `Resolution` whose notes name how many
  files move with each module that carries more than its own; `backends/rust.rs` calls them in two places,
  line-neutral.
- **Tests.** New `tests/crate_move_children.rs` and `tests/crate_move_restricted_declarations.rs` (library
  level, no server); additions to `tests/check_precondition_parity.rs`, `tests/move_module_to_crate_acceptance.rs`
  and `tests/cluster_move_acceptance.rs` (both live binaries were already registered in
  `.config/rust-e2e.filterset` and the `rust-analyzer` group); unit tests beside `manifest_edits`, `module_files`,
  `source_scan` and `moving`.
- **Behaviour changed for existing plans.** A glob facade over a `pub(crate)`, `pub(super)`, `pub(in …)` or
  private `mod` is written with that visibility instead of `pub`; plans that named a directory child in
  `also` now nest it under its parent instead of flattening it to the destination's root.
  `crate_move::tests::leaves_a_glob_facade_where_the_module_was_declared` asserted `pub use` for a private
  `mod host_registry;` and now asserts `use tddy_host_service::host_registry;`.
- **Still the engine's gaps:** a restricted child that something outside the tree reaches is refused, not
  widened; a module written `<module>/mod.rs` cannot be an anchor; an enclosing inline module's own visibility
  is not read; the cycle refusal names the module's own file. See Backlog.

## Backlog

- **Resolved, deleted:**
  - `2026-10-08-restructure-module-move-strands-its-directory-child-and-the-cluster-leaves-a-dangling-self-reexport`
    (children carried at their nested position, no self facade).
  - `2026-10-08-restructure-move-to-crate-does-not-read-a-restricted-mod-declaration` (any visibility is a
    declaration).
  - `2026-10-08-hand-widened-mod-declarations-before-engine-moves` (restricted declarations move and the
    facade keeps their visibility, so no pre-move widening is needed).
  - `2026-10-08-restructure-cluster-move-misses-body-paths-and-writes-self-referencing-test-reexports` (no
    self re-export; a `..crate::…` base is re-pointed).
  - `2026-09-09-macro-expansion-as-a-restructure-operation`, closed as already done: a research note whose
    recommendation is not to build the operation. Deleted with the developer's approval.
  - `2026-09-09-restructure-defects-from-the-connection-service-split`, closed as already done: D6 to D10 were
    fixed before this change, and the facade caveat survives as item M of
    `2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run`. Deleted with the developer's
    approval.
- **Narrowed, kept:**
  - `2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind`: the children,
    the dry-run count and the static `check` report are closed; the stranded doc comment was the tidy
    work of `#reshape` 3. What remains is the crate a target table declares (`libc`), owned by the
    `new-crate` node. The entry is retitled and keeps its file name.
  - `2026-10-08-restructure-move-cluster-ignores-also-members-that-are-directory-children-and-their-crates`:
    the `also` children and the crates they name are closed; creating the destination crate, `async-trait` and
    `libc` remain, owned by the `new-crate` node.
- **New, recorded by this change:**
  - `2026-10-09-restructure-crate-move-cannot-anchor-a-mod-rs-module`
  - `2026-10-09-restructure-crate-move-does-not-widen-a-restricted-child-module`
  - `2026-10-09-restructure-four-copies-of-the-visibility-stripper`
  - `2026-10-09-restructure-mod-declaration-with-an-attribute-on-the-same-line-is-not-read`
  - `2026-10-14-restructure-restricted-child-read-ignores-an-enclosing-inline-modules-visibility`
  - `2026-10-14-restructure-cycle-refusal-names-the-modules-file-not-the-carried-child-that-holds-the-path`
- **Touched, not fixed:** `2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header` (this
  change reads the header of every carried file; widening it past the header is `#reshape` 7's).

## Verification (scoped to `tddy-code-restructuring`)

`cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean on the
final tree. Targets run: `--lib` filtered to `crate_move` (268 passed), `crate_move_children` (18),
`crate_move_restricted_declarations` (9), `check_precondition_parity` (23), live `cluster_move_acceptance` (5),
live `move_module_to_crate_acceptance` (4), and `apply_compile_gate_acceptance` (7, under
`CARGO_TERM_COLOR=always`). The whole package ran 1,516 passed with two failures in other nodes' tests
(`extract_method_clean_acceptance`, `extract_method_guard_lift_acceptance`) that pass alone in about ten
seconds, so they were load-related. Whole-workspace health is CI's.

## Code issues (re-measured 2026-10-14)

None was claimed by #602. No record is closed or narrowed.

| Record | Measurement | Verdict |
|---|---|---|
| `oversized-file-backends-rust` | 2,997 production lines to the first inline `#[cfg(test)] mod`, against 2,999 at the merge base; two call sites swapped | touched, not grown; row recorded there; open |
| `oversized-file-test-binary` | 967 production lines (no `#[cfg(test)]`), unchanged | not touched; open |
| `complexity-rust-facade-lines`, `dead-code-plan-filehint-modified`, `broken-restructure-anchors-empty-outline` | not touched | open |

File lengths, to the first `#[cfg(test)]` at line start: `carried.rs` 389, `cluster.rs` 250 (from 260 at the
merge base), `cluster/gathering.rs` 80, `manifest_edits.rs` 470, `moving.rs` 371, `facade_writer.rs` 416,
`stranded.rs` 395, `preconditions.rs` 298, `source_scan.rs` 357, `crate_move.rs` 437; every touched file is
under the 500-line budget.
