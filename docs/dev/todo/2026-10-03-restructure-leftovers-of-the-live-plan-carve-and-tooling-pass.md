# 2026-10-03 — what the `#live-plan` 7/15 carve and tooling pass left open

**Category:** Technical debt and follow-ups, one entry because they share one cause: the carve of the
oversized files ran the restructure engine against real code for the first time at this size, and each
real run found a defect that was fixed on the spot. These are what was *not* done, or not provable.
**Source:** `#live-plan` 7/15, [#539](https://github.com/uppin/tddy-coder/pull/539). The fixes made on the
way (T1–T24) are in that PR's change history; this lists only what remains.

## 1. `backends/rust.rs` — the impl-member seams (still ~2.6k production lines)

The free-item runs are done (nine modules: `line_diff`, `placeholder_checks`, `lsp_edits`,
`import_text`, `module_text`, `visibility`, `seam_survey`, `facade`, `server_process`; `rust.rs` went
from 4,475 to 2,666 production lines). What remains is the methods of the three `impl RustBackend`
blocks and the trait impls, which only the impl-member seam of `extract_module` can move:

| Run | Members | Lines |
|---|---|---:|
| `session` | `with_progress` … `incomplete_index` (keep `new`) | ~118 |
| `transport` | `start` … `assist` | ~320 |
| `extraction` | `assisted_edit` … `reach_of` | ~334 |
| `assist_ops` | `prune_assist_imports` … `rename_placeholder` | ~332 |
| `backend_impl` | the three whole trait impls (`Drop`, `LanguageBackend`, `ModuleReferences`) | ~132 |

Not done because each needs a probe first (`assist_ops`: `check --deep` and `apply --dry-run`), because a
moved private method comes out `pub(crate)` and **stays** so (about 25 methods, plus the fields of 13
structs), and because the empty-`impl` shell case is unverified. Close it with the carve's usual plan
sequence, then delete `packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md`
recording the final number.

## 2. The facade over-lists (upstream of the tidy)

A `named` facade re-exports every item "reached from outside the new module", and the parent's own
tests count as outside, so a split leaves wide `use module::{…}` groups of which most names are unused in
the library build (13 findings over 11 groups for `plan-rust1`). The tidy now cleans them
(root cause in `docs/dev/changesets/2026-10-03-index-daemon-live-plans.md`), but the engine
should not write them. Design (60–90 lines in `rust.rs` plus tests): `reach_of` records whether any
same-file reference lies outside a `#[cfg(test)]` module; `MovedItem` carries `reached_from_production`;
`facade_lines` / `facade_will_bind` emit production-reached names as today and the rest under
`#[cfg(test)]`. A reference in another file still counts as production. Do it **after** item 1, so the
code it touches is in its final place.

## 3. `crate_move/test_binary.rs` (966 lines, pre-existing)

Not a regression of the stack, so not carved here. The analysis found three contiguous seams — `prose`
(`Prose`…`character_literal_end`), `names_bound` (after `prose` is out, two runs become adjacent) and
`facade_walk` — taking the parent to ~417. Its record is
`packages/tddy-code-restructuring/docs/code-issues/oversized-file-test-binary.md`.

## 4. A stale plan after a failed apply says only "item changed"

`apply` rewrites the plan file as it runs, so a plan whose run failed and was rolled back is stale, and
re-running it is refused with `the item … changed since the plan was written — re-anchor it`, which
sends the author looking for an edit they did not make. The refusal should say that the plan file was
last written by a run (the journal under `.restructure/` knows) and that the remedy is to regenerate it.
The skill now documents the behaviour (`code-restructuring` § Testing cadence); the message does not.

## 5. Untested shapes in the fixes

- **Field and method visibility.** A moved struct's `pub(super)` field is rebased (block-form bodies
  only); a tuple struct whose fields are on one line, and `pub(super)` **methods** inside a moved `impl`
  block, are not covered and were not tested.
- **A trait import only tests use.** The tidy gates an import by the *names the errors quote*; an error
  that names a method (`write_str`) never names the trait, so `use std::fmt::Write;` needed only by
  tests fails the run loudly (the tidy is undone) instead of being gated. Matching the `help` children
  (`trait Write`) would close it.
- **Round-by-round iteration** of the tidy's repair is tested at the decision level only; no cheap
  real-cargo case newly quotes names in a second round.
- **`restructure_args` / daemon `--items`.** Four front ends now share `parse_item_list`; a fifth
  (anything that builds an `AnchorsRequest` by hand) would have to call it too.

## 6. `verify` reports one `)` lost when a call is reflowed into a block

The token pass drops the structural closer lines (`})`) before it counts tokens, so a call rustfmt
turned from `.filter(|m| …)` into `.filter(|m| { … })` reports one `)` lost although nothing is. It
appeared on the `rust.rs` split. Count the parentheses and brackets of the excused structural lines on
both sides in the token pass, so layout moves them without changing the balance.

## 7. A split drops a `pub` item nothing references from the crate's public path

A named facade lists the items "reached from outside the new module". An item declared `pub` that
nothing in the workspace calls is not reached, so it is left out, and the moved `pub fn` ends up
public in a *private* module: unreachable by any path, and a dead-code warning on the library build.
It showed on the `verify.rs` split (`pub fn statements`, no callers); the tidy reported the warning and
the apply was clean otherwise. Fixed there by hand (one `pub use`, marked as a post-move fix).

**What would close it.** `facade_lines` / `facade_will_bind` must include every item the moved run
declares `pub` (and, for a `pub(crate)` run, `pub(crate)` items another module of the crate can see),
whether or not the reference survey finds a caller — the public path is part of the crate's interface,
and removing it is not something a move is allowed to do. Test: extract a contiguous run containing an
unreferenced `pub fn` with `reexport: named` and assert it is still reachable through the old path.

## 8. Process: tooling fixes were validated on the failing split, not on a corpus

Each of T7b, T15, T19–T24 was found by a real split and proven by a fixture written after the fact.
There is no corpus of real splits the engine is replayed against, so a change to the import or
visibility passes can still break a shape nobody has re-run. The preserved failing trees
(`fail-*` in the working scratch, not committed) are the seed: turn them into `tests/fixtures/` replays
that run `check --deep` and `apply` against a trimmed copy of the real file.
