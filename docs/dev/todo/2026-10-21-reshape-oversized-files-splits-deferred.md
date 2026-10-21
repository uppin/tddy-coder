# 2026-10-21 — Five engine files over the 500-production-line budget: the splits are deferred

**Category:** Deferred from `reshape-oversized-files` (`#reshape` 17/24, PR #612)
**Source:** [2026-10-09-reshape-oversized-files.md](../1-WIP/2026-10-09-reshape-oversized-files.md) — Plans A–D and the `test_modules.rs` split
**Owner:** the `#reshape` sweep node

## Why deferred

The developer deferred the five file splits on 2026-10-21 ("we can defer those"). What landed in #612 is the
counter and the `restructure lines` subcommand (`30d514d0c`): the production-line count now leaves out test-only
items, so these five files are measured correctly and are known to be over the budget. The splits themselves were
not applied, and no move was made by hand.

## The deferred files

Production lines by the items rule (`production_lines_of_file`, measured at the `#reshape` 17/24 base on 2026-10-21):

| File | Production lines | Planned seams | Plan |
|---|---:|---|---|
| `packages/tddy-code-restructuring/src/crate_move/test_binary.rs` | 967 | `src/lexical.rs` (new crate-root leaf, ~154: `Prose`, `readable_spans`, `block_comment_end`, `literal_end`, `raw_string_hashes`, `string_end`, `character_literal_end`); `crate_move/source_names.rs` (new, ~233); `crate_move/test_binary/facade_walk.rs` (new child, ~91: `Defining`, `defining_home`, `crate_past_the_facades`); `crate_move/test_binary/dev_dependencies.rs` (new child, ~95). `test_binary.rs` keeps ~394. Callers re-pointed first: `source_scan`, `header`, `survey`, `early_return`. Plan A |
| `packages/tddy-code-restructuring/src/backends/rust/item_move/assemble.rs` | 521 | `item_move/moving.rs` (new, ~25: `Moving`); `item_move/visibility.rs` (new, ~137: `Landing`, `spelled`, `read_scope`, `visibilities`, `widen_reached_items`, `users_of`). `assemble.rs` keeps ~356. **`widen_reached_items` is not in the changeset's State B list but `visibilities` calls it, so it moves with them.** Plan B |
| `packages/tddy-code-restructuring/src/runner/tidy.rs` | 542 | `runner/tidy/fixes.rs` (new, ~114: `UnusedImports`, `unused_imports`, `read_by_a_unit`, `apply_fixes`, `refuse_unappliable`), move first; `runner/tidy/rounds.rs` (new, ~230: `Report`, `Round`, `Repairs`, `tidy_imports`, `begin`, `Repair`, `MAX_REPAIRS`, `repair`, `undone`, `restore`), move second. `tidy.rs` keeps ~196. Plan C |
| `packages/tddy-code-restructuring/src/verify/retarget.rs` | 631 | `verify/retarget/delegators.rs` (new): `Forwarded`, `delegators`, `signature_of`, `forwarding_call`, `opening_of_last_paren`, `matching_close`, `split_top_level` (the R3 delegator pass, as the code-issue record suggests); and `Declared::with_detaches` moves to the existing `verify/detach.rs` by `move_impl_members`. Plan D |
| `packages/tddy-code-restructuring/src/crate_move/test_modules.rs` | 664 | **Not decided.** Not in the changeset's State B, so it needs its own plan and seams. Candidate groupings by name only, not yet checked against call edges: declaration reading (`declared_file`, `placed_by_attribute`, `spanned_files`, `read_paths`, `sorted`, `test_gated`, `declared_under_test`, `inline_scope`); following a move (`FollowingTest`, `following_of`, `travelling`, `notes_of`, `follow_changes`, `followed`, `landing_collision`); references from outside (`Outside`, `kept_by_outside_references`, `reaching`, `inside_the_module`, `spelled_at`, `unfollowable`). The `test_modules` split |

Order that the plans need: Plan A's `lexical` before its `source_names` (both read it); Plan B's `moving` before
`visibility`; Plan C's `fixes` before `rounds`. Each is a `tddy-tools restructure` plan: anchors, then
`check --deep`, then `--dry-run`, then apply, then `verify --against HEAD`, one commit per plan.

## What the deferral leaves in the code

- `packages/tddy-code-restructuring/tests/engine_file_budget_shape.rs`: `NOT_YET_SPLIT` lists the five files. Remove
  each line when its split lands.
- `packages/tddy-code-restructuring/tests/engine_module_edges_shape.rs`: the six edge tests are `#[ignore]`d with a
  pointer here. They read files the splits create (`src/lexical.rs`, `source_names.rs`, `item_move/moving.rs`,
  `item_move/visibility.rs`, `runner/tidy/fixes.rs`) or check edges those splits remove. Un-ignore each test when the
  split it reads lands:
  - `the_lexical_masker_depends_on_no_engine_module`, `the_rust_backend_does_not_reach_crate_move_for_the_masker`,
    `nothing_but_crate_move_names_test_binary`: Plan A.
  - `item_move_siblings_do_not_import_from_assemble`, `the_record_of_a_move_does_not_reach_the_modules_that_read_it`:
    Plan B.
  - `tidy_parts_do_not_import_the_rounds`: Plan C.
- The code-issue records `packages/tddy-code-restructuring/docs/code-issues/oversized-file-test-binary.md` and
  `oversized-file-verify-retarget.md` are still open. The record for `test_modules.rs` does not exist.
- `docs/dev/todo/2026-10-06-restructure-item-move-assemble-past-500.md` had `Claimed by: #612`. That claim is withdrawn
  because #612 no longer splits `assemble.rs`, and this entry now owns it.
- `docs/dev/todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md` item 3 (`test_binary.rs`
  size) stays open.

## Not affected

- `backends/rust.rs` (about 2,832 production lines) stays exempt, owned by
  [2026-09-16-backends-rust-rs-is-4500-production-lines.md](2026-09-16-backends-rust-rs-is-4500-production-lines.md).
- The counter and `restructure lines` are done and not reopened here.
