# Changeset: facades keep their public path, crate moves remove a declaration whole, and the tidy leaves a lint-clean origin

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Bug fix (engine defects found by real restructures; no new operation, plan field or wire message)
**Stack**: `#reshape` 3/19, branch `feature/reshape/tidy-facades`, green wave 1. PR title:
`fix(code-restructuring): facades keep their public path and the tidy leaves a lint-clean origin (#reshape 3/19)`.
Base in the linear stack: `feature/reshape/multi-seam-extract` (K=2). **Real edges**: none in (this node consumes no
other node's behaviour); out: `tidy-facades -> oversized-files` (K=15) and `tidy-facades -> rust-backend-split` (K=17).

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-09-reshape-tidy-facades-initial-discovery.md)
(Exploration 1 is the whole-work discovery; Exploration 2 is this node's).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

`grep -rl 'Claimed by:'` over `packages/tddy-code-restructuring/docs/code-issues/` finds no issue claimed by an open
PR in the path: **no 🚧 claimed issue, no wait-or-proceed fork.**

| Item | Verdict | What this change does about it |
|---|---|---|
| [2026-10-03-restructure-glob-reexport-is-narrower-than-the-moved-items-need.md](../todo/2026-10-03-restructure-glob-reexport-is-narrower-than-the-moved-items-need.md) | ✅ **RESOLVED HERE** | The width rule is already right (`facade.rs:124-130`); its input was an empty survey. The surveys read a settled outline and an empty survey over declared items is refused (F1). Its second half (tidy skipped on a failed gate) is the gate note. Deleted at wrap; tests 7-8, 10, 25 |
| [2026-10-08-restructure-cluster-move-leaves-unused-reexports-and-an-orphan-doc-in-the-origin.md](../todo/2026-10-08-restructure-cluster-move-leaves-unused-reexports-and-an-orphan-doc-in-the-origin.md) | ✅ **RESOLVED HERE** | A glob only tests read is gated in the repair; a crate move removes a declaration with its docs, which travel to the destination. Deleted at wrap; tests 13-15, 17-18, 22-23 |
| [2026-10-08-restructure-apply-rustfmt-reorders-unrelated-reexports-in-the-origin-root.md](../todo/2026-10-08-restructure-apply-rustfmt-reorders-unrelated-reexports-in-the-origin-root.md) | ✅ **RESOLVED HERE** | Its stated cause ("`lib.rs` was not rustfmt-sorted") is wrong — CI gates `cargo fmt --check`. The removed `mod` line joined two `use` runs. A removal or facade between two runs keeps them apart. Deleted at wrap; tests 16, 19-20 |
| [2026-10-08-restructure-move-item-miswrites-a-facade-path-in-a-moved-impl-block.md](../todo/2026-10-08-restructure-move-item-miswrites-a-facade-path-in-a-moved-impl-block.md) | partial (tidy half) | A failed compile gate says the tidy did not run and counts the unused imports it left (F2). The `super::<crate>::` path is `#reshape` 11's (`feature/reshape/move-item-paths`), which deletes the file; this wrap narrows it to that half |
| [2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md](../todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md) | partial (items 2, 7) | Item 7: a named facade keeps every `pub` item. Item 2: test-only names on a `#[cfg(test)]` line. Item 5's second bullet (a trait import only tests use) is closed as a side effect of the repair rule. Wrap narrows the file to items 1, 3, 4, 5 (minus that bullet), 6, 8; tests 1-5, 11-12, 24 |
| [2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md](../todo/2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md) | partial (the stranded doc comment) | Claimed by `#reshape` 5 (`feature/reshape/move-children`). Its "doc comment stranded on the next item" finding is the same mechanism and is fixed here; the claimant's wrap drops that sentence |
| [`complexity-rust-facade-lines.md`](../../../packages/tddy-code-restructuring/docs/code-issues/complexity-rust-facade-lines.md) | ⚠ **DURING** | Claimed by `#reshape` 19. The named arm of `facade_lines` moves into a new `named_facade_lines`, so the function's depth falls; a measurement row is added at wrap, and node 19 decides closure |
| [`oversized-file-backends-rust.md`](../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md), [2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md](../todo/2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md) | ⚠ **DURING** | `rust.rs` changes: two outline calls, `reach_of`'s test-module test, two `MovedItem` literals; net ≤ +10 production lines, and `attached_trivia_starts_at` (18 lines) leaves it. History row at wrap |
| [`broken-restructure-anchors-empty-outline.md`](../../../packages/tddy-code-restructuring/docs/code-issues/broken-restructure-anchors-empty-outline.md) | — Related | `#reshape` 12's. This node makes the surveys use `settled_outline` as it stands; it does not change that function |
| [2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md](../todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md) | — Unrelated (node 15) | The counter under-reads `runner/tidy.rs` (~540 real production lines). Per the developer's decision, the counter and `tidy.rs`'s size are `#reshape` 15's; this node adds no production line to `tidy.rs` |
| [2026-09-24-restructure-apply-leaves-the-lint-gate-red.md](../todo/2026-09-24-restructure-apply-leaves-the-lint-gate-red.md) | — Unrelated | N3 (missing imports) is `#reshape` 4's; the tidy's removal rules are not changed |

## Affected Packages

- **`tddy-code-restructuring`**:
  - `src/backends/rust/facade.rs` (named rule, `named_facade_lines`) + new `src/backends/rust/facade_tests.rs`;
  - `src/backends/rust/seam_survey.rs` (`MovedItem.reached_from_production`, `Reach.from_production`,
    `refuse_unsurveyed_range`, `test_module_lines`) + new `src/backends/rust/seam_survey_tests.rs`;
  - `src/backends/rust.rs` (`survey_moved_items`, `survey_impl_members` read `settled_outline`; `reach_of` takes the
    test-module lines; `attached_trivia_starts_at` leaves, by an engine move, for `src/crate_move/source_scan.rs`);
    `src/backends/rust/impl_seam.rs` (one `MovedItem` literal);
  - `src/crate_move/manifest_edits.rs` (`RemovedDeclaration`, `removed_declaration`, `separates_use_runs`,
    `insert_module_declaration_sorted` takes the docs), `src/crate_move/moving/facade_writer.rs` (`leaving`,
    `declared_in_destination`), `src/crate_move/preconditions.rs` (`move_preconditions` reads the declaration whole);
  - `src/runner/tidy/gating.rs` (`to_gate_in_a_repair`); `src/runner/tidy.rs` (one call swapped at `:328`, tests added
    after its production code);
  - `src/runner/compile_gate.rs` (`Rejection`, `untidied_note`), `src/lib.rs` (`AppliedTreeDoesNotCompile.untidied`);
  - tests: new `tests/extract_facade_acceptance.rs`; `tests/move_facades_acceptance.rs`,
    `tests/apply_compile_gate_acceptance.rs`, `tests/harness/mod.rs`.
  - Docs at wrap: [facades.md](../../../packages/tddy-code-restructuring/docs/facades.md),
    [readiness-and-gates.md](../../../packages/tddy-code-restructuring/docs/readiness-and-gates.md) (§ The tidy,
    § Known limitations), [docs/ft/coder/rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md)
    (`### Import restoration` facade paragraph, `## The tidy`, the crate-move rows of `## Rust operations (v1)`).
- **`.config/rust-e2e.filterset`, `.config/nextest.toml`**: `binary(extract_facade_acceptance)` in both.
- **`tddy-index-daemon`**: no source change (`status.rs:79` matches `AppliedTreeDoesNotCompile { .. }`).
- **`tddy-tools`**: no source change.

## Related Feature Documentation

- [PRD-2026-10-09-reshape-tidy-facades.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-tidy-facades.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md) — `### Import restoration`, `## The tidy`,
  `## Rust operations (v1)`

## Summary

An `extract_module` facade no longer narrows or drops a public path: the survey behind it waits for a settled outline
and refuses an empty answer over declared items; a `named` facade keeps every `pub` item and writes the names only
the file's tests reach under `#[cfg(test)]`. A crate move removes a `mod` declaration with the comments above it —
which travel to the destination's declaration — refuses one carrying another attribute, and never joins two `use`
runs. The tidy's repair gates whatever one build reads and another reports unused, globs and trait imports included.
A failed compile gate says the tidy did not run.

## Background

Each defect cost a hand edit after an otherwise clean `apply`: `pub(crate)` → `pub` on two `#live-plan 10/15` globs;
a re-added `pub use` after the `verify.rs` split; eleven test-only facade groups the tidy had to rewrite on `#539`;
eight unused globs and stranded doc comments in `#carve 21/21` (one still on master,
`tddy-session-lifecycle/src/lib.rs:101-103`); a 17-line re-sort around a one-line facade; and a hand `cargo fmt` /
`cargo fix` after every failed gate, recorded in four #536 commit messages.

## Responsibility

- **Surveys**: `survey_moved_items` and `survey_impl_members` read `documentSymbol` through `settled_outline`;
  `survey_moved_items` refuses a range that declares a named item or child module when the survey found none.
- **Named facade**: every `within.is_empty()` item written `pub` is carried; a narrower one only when
  `reached_from_outside`. Production-reached names (and every `pub` item) go on the tier lines as today; names reached
  only from the file's own `#[cfg(test)]` module go on tier lines preceded by a `#[cfg(test)]` line, after the
  production lines. `facade_will_bind` mirrors "carried".
- **`reach_of`**: a same-file reference outside the range sets `from_production` unless it lies inside a
  `#[cfg(test)] mod … { }` of the file; a reference in another file always sets it.
- **Crate-move declaration**: the removed span runs from the first of the comment and attribute lines directly above
  `mod <module>;` (no blank line between) to the end of its line. Doc and plain comments travel to the destination's
  `pub mod <module>;`, inserted above it; any `#[…]` line refuses the operation (static and resolve), naming it.
- **`use` runs**: when the line above the removed span ends a `use` item and the line below starts one (or its doc),
  an empty replacement becomes `"\n"` and a facade replacement is followed by `"\n"`.
- **Tidy repair**: the spans gated in a repair are those `named_by_errors` matches **plus** every span in
  `read_by_a_unit` — evidence that does not need a quoted name.
- **Gate failure**: `AppliedTreeDoesNotCompile` carries `untidied`: "the tidy did not run, so the written files were
  neither tidied nor formatted; the failing check reported N `unused import` warning(s) in them" (N from the gate's
  `short` stderr, counted over touched files).
- Register `tests/extract_facade_acceptance.rs` in `.config/rust-e2e.filterset` and the `rust-analyzer` group.

## The rules (the contract)

**1. Survey refusal** (`seam_survey::refuse_unsurveyed_range`). With `range_text` the original lines of the range:
`items_of_module(range_text)` (`crate_move/source_scan/module_items.rs:32`) lists at least one `defined` name or
`child` module, and `moved` is empty → `seam_refusal`:
`` the range <start>-<end> of <file> declares `<first name>` (and N more), and the server's outline lists none of them — a facade written now would name nothing. Re-run once the index has loaded ``.
A range of `impl` blocks only lists nothing in `items_of_module` and is not refused.

**2. Named facade lines** for `module` `m` (order is the contract):

```
pub use m::{a, b};                 # pub items, carried whether or not reached
pub(crate) use m::{c};             # reached from production, by visibility tier, widest first
use m::{d};
#[cfg(test)]
pub(crate) use m::{t};             # reached only from the file's own #[cfg(test)] module
```

A `pub` item reached only from tests stays on the `pub` line (its public path is interface). An empty result keeps
`empty_facade_note`. `refuse_uncovered_nesting` is unchanged. The glob rule is unchanged.

**3. Declaration removal** (`manifest_edits::removed_declaration`):

| Lines directly above `mod m;` | Result |
|---|---|
| none, or a blank line | span = the declaration's line (today's span) |
| `///` / `//` lines | span starts at the first; those lines travel above the destination's `pub mod m;` |
| any `#[…]` (incl. `#[doc = …]`, `#[cfg]`, `#[path]`, `#[allow]`) | refused: `` `mod m;` in <file> carries `<attribute>`, which a move to another crate can neither keep nor drop without changing what compiles — remove or move it by hand, then plan again `` |

**4. Use-run separation** (`manifest_edits::separates_use_runs(text, span)`): true iff the nearest non-blank line
above `span` is the last line of a `use` item and the first line below `span` starts a `use` item or a comment line
directly above one. `leaving` then writes `"\n"` for an empty replacement and `"<facade>\n\n"` for a facade.

**5. Repair evidence** (`gating::to_gate_in_a_repair`): `named_by_errors(...)` ∪ `unused.read_by_a_unit`, deduplicated,
in `Span` order. `MAX_REPAIRS`, `keeps_improving` and the undo-and-fail path are unchanged.

## Boundaries

- No change to the glob width rule, to cross-crate facade lines (`facade_lines_for_plan`), to `refuse_stranded`, or
  to `restore_visibility`.
- No change to `settled_outline` itself (node 12's) or to `request_settled`.
- No change to which declarations `module_declaration` matches (node 5 changes that); `removed_declaration` calls it
  and widens the span upward.
- The tidy still runs only on a compiling, complete run; no import is removed from a broken tree; `rustfmt` still
  formats whole files; no line-range formatting.
- `module_reparent` and `item_move` removals are not given the use-run rule (proposed todo).
- No production line is added to `runner/tidy.rs`; no function on the >60-line list (whole-work discovery
  Exploration 3) grows: `assisted_edit` and `resolve_cluster` are not edited; new logic sits in new functions.
- The one engine move (`attached_trivia_starts_at` → `crate_move/source_scan.rs`) is made with `tddy-tools
  restructure` only; hand edits only to fix the build after it, each with a todo entry; a refusal stops and asks the
  developer.

## Dependencies

This node consumes no other node's behaviour; it has no parent row. **Line-order facts** (textual collisions only,
resolved by rebasing in line order, never by consuming): `#reshape` 5 (`feature/reshape/move-children`) edits
`crate_move/manifest_edits.rs` `module_declaration` (which declarations match — restricted visibilities) and
`facade_writer.rs`; this node changes the **span removed** around a match, on top of whatever `module_declaration`
returns. `#reshape` 2 (`feature/reshape/multi-seam-extract`, the base) may edit `survey_moved_items` / `reach_of` in
`backends/rust.rs`. `#reshape` 12 edits `settled_outline`, which this node calls unchanged.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| — | — | — | — |

## Draft PR contract

Published with the wave-2 contract commit (the first push of this PR, stubs that compile and fail); **owned surface**:

- `backends::rust::seam_survey`: `MovedItem.reached_from_production: bool`; `Reach.from_production: bool`;
  `pub(crate) fn refuse_unsurveyed_range(range_text: &str, file: &str, range: Range, moved: &[MovedItem]) -> Result<()>`;
  `pub(crate) fn test_module_lines(text: &str) -> Vec<std::ops::RangeInclusive<u32>>`.
- `backends::rust::facade`: private `fn named_facade_lines(module: &str, items: &[seam_survey::MovedItem]) -> Result<Vec<String>>`.
- `crate_move::manifest_edits`: `pub(crate) struct RemovedDeclaration { pub(crate) span: std::ops::Range<usize>, pub(crate) comments: String }`;
  `pub(crate) fn removed_declaration(text: &str, path: &str, module: &str) -> Result<Option<RemovedDeclaration>>`;
  `pub(crate) fn separates_use_runs(text: &str, span: std::ops::Range<usize>) -> bool`;
  `insert_module_declaration_sorted(text: &str, line: &str, comments: &str) -> TextEdit` (new third parameter).
- `runner::tidy::gating`: `pub(super) fn to_gate_in_a_repair<'a>(unused: &'a UnusedImports, before: &BTreeMap<String, Vec<u8>>, quoted: &BTreeSet<String>) -> Vec<&'a Span>`.
- `runner::compile_gate`: `pub(super) struct Rejection { checked: String, errors: String, stderr: String }`;
  `failing_check(...) -> Result<Option<Rejection>>`; `fn untidied_note(stderr: &str, touched: &BTreeSet<String>) -> String`.
- `RestructureError::AppliedTreeDoesNotCompile { …, untidied: String }` (public enum; matched with `{ .. }` in
  `tddy-index-daemon`).
- Test harness: `harness::a_workspace_whose_test_binary_reads_a_file_beside_it_and_carries_an_unused_import`.
- Failing tests: the red ones in "Acceptance tests" (4 green pins and one reproduction are named there).

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes — it consumes no node's behaviour.
**Concurrent with:** every other wave-1 node (`widen-same-crate`, `multi-seam-extract`, `extract-method-clean`,
`move-children`, `methods-leave-type`, `move-widen`, `move-grouped-use`, `new-crate`, `apply-robust`,
`move-item-paths`, `anchors-outline`). Textual overlaps (line-order facts, not edges): `move-children` on
`crate_move/manifest_edits.rs` `module_declaration` and `facade_writer.rs`; `multi-seam-extract` on the survey and
`reach_of`; `anchors-outline` around `settled_outline`.
**Blocks:** `feature/reshape/oversized-files` (K=15), `feature/reshape/rust-backend-split` (K=17).
Real dependency edges (whole stack): `1→13`, `5→14`, `2→15`, `3→15`, `4→16`, `13→17`, `2→17`, `3→17`, `17→18`, `6→18`,
`4→19`, `17→19`. This node's: `3→15` (node 15 carves `runner/tidy.rs`, `item_move/assemble.rs` and `test_binary.rs`
with the engine, needing a tidy that does not fail over test-only globs and a crate move that leaves no orphan
docs), `3→17` (node 17's `rust.rs` split is a run of named facades that must keep `pub` items and gate test-only
names).

## Successor PRs

- `feature/reshape/oversized-files` (K=15)
- `feature/reshape/rust-backend-split` (K=17)

## Scope

- [ ] **Surveys**: `settled_outline` in both surveys; `refuse_unsurveyed_range`
- [ ] **Named facade**: `pub` always; test-only tier under `#[cfg(test)]`; `facade_will_bind`; `reach_of` production
- [ ] **Crate-move declarations**: `removed_declaration`, comment travel, attribute refusal (static + resolve)
- [ ] **Use runs**: `separates_use_runs` in `leaving`
- [ ] **Tidy repair**: `to_gate_in_a_repair`
- [ ] **Gate note**: `Rejection`, `untidied_note`, the error field
- [ ] **Engine move**: `attached_trivia_starts_at` to `crate_move/source_scan.rs` via `tddy-tools restructure`
- [ ] **Registration**: `extract_facade_acceptance` in both config files
- [ ] **Package documentation** at wrap (list under Affected Packages)
- [ ] **Testing**: acceptance tests pass; `./test -p tddy-code-restructuring`, scoped; CI for the rest
- [ ] **Code quality**: `cargo check -p tddy-code-restructuring --all-targets`, clippy `-D warnings`, `cargo fmt`; no
      touched file passes 500 production lines; no function on the >60 list grows

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

- `widest_visibility` (`backends/rust/facade.rs:124-130`) is right; `survey_moved_items` (`backends/rust.rs:1650-1676`)
  and `survey_impl_members` (`:1750-1756`) read `documentSymbol` through `request_settled` (`:894-913`), the only
  outline readers not using `settled_outline` (`:1698-1719`). An empty survey writes `pub(crate) use m::*;` silently.
- `facade_lines` Named (`facade.rs:170-206`) and `facade_will_bind` (`:136-149`) carry only `reached_from_outside`;
  `reach_of` (`rust.rs:1833-1872`) counts a parent-test reference as outside.
- `module_declaration` (`crate_move/manifest_edits.rs:6-19`) spans the `mod` line alone; `leaving`
  (`crate_move/moving/facade_writer.rs:84-100`) replaces it with the facade or nothing; `declared_in_destination`
  (`:291-313`) writes a bare `pub mod m;`. Comments and attributes above stay, attached to the next item; removing the
  only line between two `use` runs joins them and rustfmt (`runner/tidy/format.rs:54-75`) re-sorts the joined run.
- `repair` (`runner/tidy.rs:321-355`) matches by `named_by_errors` (`tidy/gating.rs:67-76`); `bound_name`
  (`gating.rs:57-64`) is `None` for a glob, so a glob only tests read fails the run over a compiling tree.
- `refuse_a_broken_result` (`runner/compile_gate.rs:91-143`) skips the tidy on a failing gate without saying so.

### State B (Target)

The rules above hold; every #536 / #539 / #566 hand edit listed in Background is unnecessary.

### Delta (What's Changing)

#### `tddy-code-restructuring`
- **`backends/rust/seam_survey.rs`** (237 → ~300): two fields, `refuse_unsurveyed_range`, `test_module_lines`.
  New `seam_survey_tests.rs`.
- **`backends/rust/facade.rs`** (262 → ~300): `named_facade_lines` takes the Named arm (depth falls); test-only tier;
  `facade_will_bind` predicate. New `facade_tests.rs` (`#[cfg(test)] mod facade_tests;`).
- **`backends/rust.rs`**: two `request_settled("textDocument/documentSymbol", …)` → `self.settled_outline(uri)`; the
  refusal call at the end of `survey_moved_items`; `reach_of` gains a `tests: &[RangeInclusive<u32>]` parameter;
  `MovedItem` literals (`:1666`, `:1792`, test helpers `:3458`, `:3470`) and `impl_seam.rs:158` gain the field.
  `attached_trivia_starts_at` (`:2426-2443`) leaves by engine move; its five callers import it from `crate_move::source_scan`.
- **`crate_move/manifest_edits.rs`** (365 → ~450 incl. tests): `RemovedDeclaration`, `removed_declaration`,
  `separates_use_runs`; `insert_module_declaration_sorted` third parameter (two test call sites).
- **`crate_move/moving/facade_writer.rs`**: `declaration_of` → `removed_declaration`; `leaving` applies rule 4 through a
  new helper; `declared_in_destination` reads each member's comments from its declaring file.
- **`crate_move/preconditions.rs`**: `move_preconditions` calls `removed_declaration` so plain `check` reports rule 3's
  refusal.
- **`runner/tidy/gating.rs`**: `to_gate_in_a_repair`. **`runner/tidy.rs`**: `:328` calls it instead of
  `named_by_errors` (same line count); tests appended inside its existing `mod tests`.
- **`runner/compile_gate.rs`** (429): `Rejection`, `untidied_note`; `refuse_a_broken_baseline` and
  `refuse_a_broken_result` destructure `Rejection` (the latter stays ≤ 60 lines; the note is built in a new function).
- **`lib.rs`**: `untidied` field and one `{untidied}` in the message.

## Implementation milestones

- [ ] **M1** named facade rules and `facade_will_bind`, `named_facade_lines`; tests 1-6
- [ ] **M2** `test_module_lines`, `reach_of` production; tests 4, 9
- [ ] **M3** surveys on `settled_outline`, `refuse_unsurveyed_range`; tests 7-8, then the live suite 10-12 and its registration
- [ ] **M4** engine move of `attached_trivia_starts_at` (`tddy-tools restructure`, `check --deep` first); build green
- [ ] **M5** `removed_declaration`, comment travel, attribute refusal (static + resolve), `separates_use_runs`; tests 13-21
- [ ] **M6** `to_gate_in_a_repair`; tests 22-24
- [ ] **M7** gate note; tests 25-26
- [ ] **M8** docs staged, scoped gate, length gate

## Testing plan

### Testing Strategy

**Primary: unit level, no server** — facade lines, survey refusal, declaration spans and use-run separation are pure
text functions; the tidy's repair is exercised against a real `cargo check` of a tempdir crate (`runner/tidy.rs`'s
`a_crate_with` / `tidied_touching`, `:549-612`), no language server.
**Live where only rust-analyzer can produce the input**: `extract_module` facades (new suite) and crate moves
(`move_facades_acceptance.rs`, already registered), with `cargo check --all-targets` and rustfmt cleanliness as oracles.

#### Option 1 (chosen): unit tests beside the code, one new live suite
**Trade-off**: the survey's outline wait is not reproduced at unit level (`fake_lsp` cannot drive `extract_module`);
the refusal is, and the live test 10 pins the end-to-end result.

#### Option 2 (rejected): extend `fake_lsp` with scripted `codeAction` answers
Would let test 10 be deterministic, at the cost of a fake rust-analyzer assist protocol in `tddy-lsp` — not this node's.

### Coverage Requirements

- [ ] Happy: glob `pub`, named `pub` unreferenced, named test-only tier, comment travel, separated runs, glob gated
- [ ] Refusals: empty survey over declared items; attribute on a crate-moved declaration (check and apply)
- [ ] Unchanged: glob width; `pub(crate)` unreferenced still left out; glob nobody reads still removed; success
      message unchanged
- [ ] Actual effects: bytes on disk, `cargo check --all-targets`, `rustfmt --check`

## Acceptance tests

Items are **red on `master`** unless marked: green pins (2, 6, 23, 26) specify what already holds; test 10 is a
**reproduction** — the cold path may already be green on master, in which case it stays as a guard and test 7 carries
the red evidence for the glob defect.

### `packages/tddy-code-restructuring/src/backends/rust/facade_tests.rs` (new; unit, `MovedItem` literals)

1. `a_named_facade_re_exports_an_unreferenced_pub_item_on_the_pub_line` — *red*: `facade.rs:172-175` drops it.
2. `a_named_facade_still_leaves_out_an_unreferenced_pub_crate_item` — green pin.
3. `a_named_facade_writes_names_only_tests_reach_under_cfg_test_after_the_production_lines` — the exact lines of rule 2.
   *Red*: no `reached_from_production` field.
4. `a_pub_item_reached_only_from_tests_stays_on_the_pub_line` — *red*: same.
5. `a_named_facade_binds_every_name_it_writes_including_unreferenced_pub_and_test_only_names` (`facade_will_bind`) —
   *red*: an unreferenced `pub` item is not bound today.
6. `a_glob_facade_is_pub_when_anything_moved_is_pub_and_pub_crate_otherwise` — green pin of `widest_visibility`.

### `packages/tddy-code-restructuring/src/backends/rust/seam_survey_tests.rs` (new; unit)

7. `a_range_declaring_items_whose_survey_found_none_is_refused_naming_the_range_and_the_first_item` — *red*: no refusal exists.
8. `a_range_holding_only_impl_blocks_is_not_refused_for_an_empty_survey` — *red* (function missing); specifies the exemption.
9. `the_test_module_lines_of_a_file_cover_its_inline_cfg_test_module_and_not_an_out_of_line_declaration` — *red*.

### `packages/tddy-code-restructuring/tests/extract_facade_acceptance.rs` (new; live rust-analyzer; `.config/rust-e2e.filterset` + `rust-analyzer` group)

10. `a_glob_extraction_of_documented_pub_items_that_lib_re_exports_leaves_pub_use_and_compiles` — fixture mirrors
    `journal.rs` / `group.rs` (`#[derive]` + `///` on `pub struct`s, `pub use parent::{A, B};` in `lib.rs`). Reproduction (see above).
11. `a_named_extraction_keeps_an_unreferenced_pub_fn_reachable_through_the_old_path` — a second crate calls
    `demo::parent::statements()` after the split and compiles. *Red*: the `pub use` is not written.
12. `a_named_extraction_gates_the_names_only_the_parents_tests_use_and_the_tidy_gates_nothing_more` — the facade holds
    a `#[cfg(test)]` line and the apply's progress has no `gated for tests:` line. *Red*: today the tidy gates them.

### `packages/tddy-code-restructuring/src/crate_move/manifest_edits.rs` (unit, existing `mod tests`)

13. `a_removed_declaration_takes_the_doc_and_plain_comments_directly_above_it` — *red*: span is one line.
14. `a_removed_declaration_stops_at_a_blank_line_above_it` — *red* (function missing).
15. `a_declaration_carrying_an_attribute_is_refused_naming_the_attribute` (table: `#[cfg(test)]`, `#[path = "x.rs"]`,
    `#[allow(dead_code)]`, `#[doc = "x"]`) — *red*.
16. `a_removal_between_two_use_runs_is_reported_as_separating_them_and_one_beside_a_blank_line_is_not` — *red*.
17. `a_declaration_is_inserted_in_sorted_position_with_its_comments_above_it` — *red*: no comments parameter.

### `packages/tddy-code-restructuring/tests/move_facades_acceptance.rs` (live, already registered)

18. `a_moved_modules_doc_comment_leaves_the_origin_and_lands_on_the_destinations_declaration` — *red*: the doc stays
    on the origin's next item.
19. `a_move_that_removes_the_only_line_between_two_use_runs_leaves_the_origins_other_lines_where_they_were` — origin
    diff is the removed declaration plus one blank line; `cargo clippy -D warnings` clean. *Red*: rustfmt re-sorts the joined run.
20. `a_facade_that_replaces_a_declaration_between_two_use_runs_leaves_the_run_below_in_place` — *red*: same mechanism.
21. `a_moved_declaration_carrying_a_cfg_attribute_is_refused_by_check_and_by_apply_and_nothing_is_written` — *red*:
    today the attribute is stranded on the next item.

### `packages/tddy-code-restructuring/src/runner/tidy.rs` (unit, existing `mod tests`, real `cargo check` of a tempdir crate)

22. `gates_a_glob_only_the_tests_read_and_the_tree_compiles_with_its_tests` — *red*: fails with "the tidy was undone".
23. `removes_a_glob_no_build_reads` — green pin.
24. `gates_a_trait_import_only_a_tests_method_call_needs` (`use std::fmt::Write;`, `out.write_str` in `mod tests`) —
    *red*: the documented known limitation.

### `packages/tddy-code-restructuring/tests/apply_compile_gate_acceptance.rs` (library `apply`, test-binary move, no server)

25. `a_failed_apply_says_the_tidy_did_not_run_and_how_many_unused_imports_it_left` — fixture
    `a_workspace_whose_test_binary_reads_a_file_beside_it_and_carries_an_unused_import`; the refusal contains
    `the tidy did not run` and `1 \`unused import\` warning(s)`. *Red*: no such text.
26. `a_successful_apply_says_nothing_about_a_skipped_tidy` — green pin.

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

Taken by the developer (2026-10-09, PRD review: "all your recommendations (F1–F5)"; brief: the budget counter and
`runner/tidy.rs`'s size go to node 15, no new code in `runner/tidy.rs`):

- **F1 — glob width**: fix the input (surveys on `settled_outline` + refuse an empty survey over declared items), not
  the rule; no width read from text or consumers.
- **F2 — failed gate**: say the tidy did not run and count the unused imports; no formatting or import removal on a
  broken tree.
- **F3 — comments on a removed declaration**: travel to the destination's declaration; any attribute is refused.
- **F4 — trivia walk**: `attached_trivia_starts_at` moves to `crate_move/source_scan.rs` by an engine move (`crate_move`
  cannot depend on `backends`); no copy.
- **F5 — leftovers item 2**: in scope now, not after node 17.

Decisions taken by this plan: test-only names follow production names (rule 2 order); a `pub` item reached only from
tests stays on the `pub` line; a facade between two runs is followed by a blank line; repair evidence is
`named_by_errors ∪ read_by_a_unit` (the known risk: a file built by a non-test second unit would be gated
`#[cfg(test)]` — no such file exists in the workspace, and the repair's re-check catches it).

**OPEN**: none.

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)
(empty)

### From @red (TDD Red Phase)
(empty)

### From @validate-changes (Change Validation)
(empty)

### From @validate-tests (Test Quality)
(empty)

### From @prod-ready (Production Readiness)
(empty)

### From @analyze-clean-code (Code Quality)
(empty)

### From @refactor (Completed Refactorings)
(empty)

## Validation Results

(empty; populated by `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`)

## TODO

- [x] Record initial discovery (`2026-10-09-reshape-tidy-facades-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-09-reshape-tidy-facades.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point: not edited while planning)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail; record whether test 10 is red or a guard)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the scoped tests (`./test -p tddy-code-restructuring`) — verify 100% pass; CI answers for the rest of the workspace
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — delete the three resolved todos, narrow the miswrite and leftovers
      entries, add the `complexity-rust-facade-lines` and `oversized-file-backends-rust` history rows, delete
      `2026-10-09-reshape-tidy-facades-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
