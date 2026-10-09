# Facades and the tidy leave a public path intact and an origin the lint gate accepts - PRD

**Date**: 2026-10-09
**PRD Type**: Bug fix (engine defects found by real restructures)
**Stack**: `#reshape` 3/19 — `feature/reshape/tidy-facades`

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `### Import restoration` (the facade paragraph: what a glob and a
  named facade contain), the cross-crate move's origin edits (`## Rust operations (v1)`, `move_module_to_crate` /
  `move_cluster_to_crate`), `## The tidy` (glob and trait imports only tests read; what a failed compile gate says).

No other feature document changes. No new operation, plan field, flag or wire message.

## Summary

Five engine defects that each cost a hand edit after an otherwise clean `apply` are fixed. An `extract_module`
facade no longer narrows or drops a public path: a glob is written from a survey that has waited for the server's
real outline, and a `named` facade keeps every `pub` item and writes the names only tests reach under
`#[cfg(test)]`. A crate move removes a `mod` declaration together with its doc comment, which travels to the
destination's declaration, and never joins two `use` runs for rustfmt to re-sort. The tidy gates a glob that only
tests read instead of failing the run over it, and an `apply` whose compile gate fails says that the tidy did not run.

## Background

The engine is now used to restructure real crates, including itself, and each run ends with a few post-move
hand edits that are always the same kind:

- `#live-plan 10/15` split `journal.rs` and `store_run.rs` with `reexport: glob`; both runs wrote
  `pub(crate) use …::*;` over items written `pub`, and the public re-exports in `lib.rs` failed (E0365, E0364).
  The rule the facade uses is right (the widest visibility of what moved); what it was handed was a survey that
  listed none of the moved items. The survey is the only outline reader that does not wait for a settled outline.
- The `verify.rs` split dropped `pub fn statements` — declared `pub`, called by nobody — from the crate's public
  path, because a named facade lists only items something references. Moving code must not change a crate's
  interface.
- `#539`'s nine-module split left eleven wide named facade groups whose names only the parent's tests use; the
  tidy now rewrites them after the fact, one `cargo check` round at a time, but the engine should not write them.
- `#carve 21/21` (#536) left eight `pub(crate) use <moved>::*;` lines in `connection_service.rs` and the doc
  comments of removed `mod` lines on unrelated items — one is still on master
  (`tddy-session-lifecycle/src/lib.rs:101-103`). Removing `pub mod agent_list_mapping;` joined two `use` runs, so
  rustfmt re-sorted unrelated lines and a one-line facade change became a 17-line diff.
- In the same stack, every apply whose compile gate failed also skipped the tidy without saying so: the hand
  `cargo fmt` and `cargo fix` that followed are recorded in four commit messages.

## Proposed Changes

### What's Changing

- **The survey behind a facade waits for the server's outline.** `extract_module` reads the file's outline the way
  every other operation does: an empty outline is believed only once the server has been seen to load. A range
  that declares items but whose survey found none is **refused**, naming the range, instead of writing a facade
  from nothing. The glob's width rule is unchanged.
- **A named facade keeps the public path.** Every moved item written `pub` is re-exported whether or not anything
  references it. A `pub(crate)` or narrower item is still re-exported only when something outside the new module
  reaches it.
- **A named facade gates the names only tests reach.** An item whose every outside reference is in the file's own
  `#[cfg(test)]` module is re-exported on its own `#[cfg(test)]` line. A reference in another file counts as
  production. The parent's own import decision (whether the facade binds a name) follows the same rule.
- **A crate move removes a declaration whole.** The doc comments (and plain comments) directly above a `mod <module>;` line go with
  it and are written above the `pub mod <module>;` the destination root gains. A declaration carrying any other
  attribute (`#[cfg(…)]`, `#[path]`, `#[allow(…)]`) is **refused** before anything is written, naming the
  attribute, because moving or dropping it changes what compiles. Plain `check` reports the same refusal.
- **A crate move never joins two `use` runs.** When the removed declaration was the only line between two runs
  of `use` items, a blank line stays in its place, so rustfmt sorts each run as it was. When a facade replaces
  such a declaration, a blank line follows the facade: the facade joins the run above it and may be moved by
  rustfmt within that run (it is the operation's own line), and the run below stays apart.
- **The tidy gates a glob only tests read.** When a round of removals breaks the re-check, every import the
  library build reports unused and another build reads — a glob, or a trait import only a test's method call
  needs — is gated `#[cfg(test)]` in the repair, even when no error quotes its name. A glob every build reports
  unused is removed, as today.
- **A failed compile gate says the tidy did not run.** The `AppliedTreeDoesNotCompile` message gains one
  paragraph: the tidy did not run, the written files were neither tidied nor formatted, and how many
  `unused import` warnings the failing check reported in them.

### What's Staying the Same

- The glob width rule (`pub` when anything moved is `pub`, else `pub(crate)`); cross-crate facades
  (`pub use <crate>::{…};`), which are already `pub`.
- The tidy's evidence (rustc's machine-applicable suggestions), its bounds (`MAX_ROUNDS`, `MAX_REPAIRS`), the files
  it may touch, and its rule that a repair which does not converge undoes itself and fails the run.
- `rustfmt` still formats every written file whole; nothing formats only some lines.
- The tidy still runs only when the compile gate passes and every operation applied; it does not run on a broken
  tree, and no fix is attempted there.
- `move_item`, `reparent_module` and their declaration handling (which already carries attributes and docs).
- A `move_item` facade path through `super::<crate>::` (the facade-path half of the 2026-10-08 miswrite entry) is a
  later node's.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring` only: `backends/rust/facade.rs` (+ a `facade_tests.rs`), the two surveys and `reach_of`
  in `backends/rust.rs`, `crate_move/manifest_edits.rs` and `crate_move/moving/facade_writer.rs`,
  `runner/tidy/gating.rs`, `runner/compile_gate.rs`, the error text in `lib.rs`. `runner/tidy.rs` does not grow:
  it is already ~540 production lines, which the budget counter under-reads; its size and the counter are
  `#reshape` 15's (`feature/reshape/oversized-files`). Here it changes by one call (same line count) and gains test lines only.
- Two new refusals can stop a plan that ran before: an empty survey over declared items, and an attribute other
  than a doc comment on a crate-moved `mod` declaration. Both name what to do.
- One live test joins an existing rust-analyzer suite; everything else is unit level or a tempdir `cargo check`.

### User Impact

- Fewer post-move hand edits: no `pub(crate)` → `pub` on a glob, no re-added `pub use` for an unreferenced item, no
  hand-deleted orphan doc, no `cargo fmt`/`cargo fix` guesswork after a failed gate.
- Reviewable diffs: a crate move's origin root shows the facade line, not a re-sorted file; a split's facade shows
  production names and test names apart.
- A run that used to fail with "the tidy was undone" over a tree that compiled now succeeds with the glob gated.

## Implementation Plan

1. Named facade rules (`pub` always, test-only names gated) and `facade_will_bind`, at unit level.
2. `reach_of` distinguishes a same-file test-module reference from a production one.
3. The surveys read a settled outline; the empty-survey refusal; one live reproduction.
4. Crate-move declaration span (doc comments, attribute refusal), doc travel to the destination, blank separator.
5. The tidy's repair gates what a build reads, globs included.
6. The compile gate's failure message names the skipped tidy.
7. Docs at wrap: feature doc, `packages/tddy-code-restructuring/docs/facades.md` and `readiness-and-gates.md`,
   the narrowed or deleted backlog entries.

## Acceptance Criteria

- [ ] An `extract_module` with `reexport: glob` over documented `pub` items that `lib.rs` re-exports publicly
      leaves `pub use <module>::*;` and the workspace compiles with its tests ([Rust code restructuring](../rust-code-restructuring.md))
- [ ] A range that declares items and whose survey finds none is refused, naming the range, and nothing is written
- [ ] A `named` facade re-exports an unreferenced `pub` item, so the old path still resolves
- [ ] A `named` facade writes the names only the file's tests reach under `#[cfg(test)]`, and the tidy finds
      nothing to gate in it
- [ ] A crate move leaves no doc comment of the moved declaration in the origin, and the destination's
      `pub mod` carries it
- [ ] A crate move of a declaration carrying a non-doc attribute is refused before any write, naming it
- [ ] A crate move that removes the only line between two `use` runs leaves the origin's other lines in place
      after rustfmt (the diff is the facade line and the removed declaration)
- [ ] A glob only the tests read is gated `#[cfg(test)]` and the apply succeeds; a glob nobody reads is removed
- [ ] A trait import only a test's method call needs is gated instead of failing the run
- [ ] An apply whose compile gate fails says the tidy did not run and how many unused imports it left
- [ ] Tests pass for `tddy-code-restructuring` (scoped; CI for the rest)

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-tidy-facades.md`
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-tidy-facades-initial-discovery.md`
- Backlog entries this resolves or narrows:
  [glob re-export is narrower than the moved items need](../../../dev/todo/2026-10-03-restructure-glob-reexport-is-narrower-than-the-moved-items-need.md),
  [cluster move leaves unused re-exports and an orphan doc](../../../dev/todo/2026-10-08-restructure-cluster-move-leaves-unused-reexports-and-an-orphan-doc-in-the-origin.md),
  [apply rustfmt reorders unrelated re-exports](../../../dev/todo/2026-10-08-restructure-apply-rustfmt-reorders-unrelated-reexports-in-the-origin-root.md),
  [move_item miswrites a facade path in a moved impl block](../../../dev/todo/2026-10-08-restructure-move-item-miswrites-a-facade-path-in-a-moved-impl-block.md) (tidy half),
  [leftovers of the live-plan carve](../../../dev/todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md) (items 2, 7)
- Package docs: [facades](../../../../packages/tddy-code-restructuring/docs/facades.md),
  [readiness and gates](../../../../packages/tddy-code-restructuring/docs/readiness-and-gates.md)
