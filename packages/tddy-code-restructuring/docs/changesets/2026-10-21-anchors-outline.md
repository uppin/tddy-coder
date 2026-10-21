# 2026-10-21 — `anchors-outline`: static prefix check, `modified` removed, `anchor_for` deleted
**Type:** Fix

- `item_anchor/prefix.rs`: `segments_below` (moved verbatim from `backends/rust/item_path.rs`) and
  `refuse_a_foreign_prefix(root, anchor)`. `check_entry_points.rs` gives each item-anchored operation one static
  finding: the first foreign-prefix refusal, or "… needs `check --deep`". The deep path calls the same rule.
  Tests: `tests/static_check_item_anchors.rs`; the pins in `repoint_call_plan_acceptance` and
  `retarget_impl_plan_lines` take the new wording (developer decision, 2026-10-21).
- `FileHint` is `{ sha256 }`; `hint_of` hashes only, so a hinted file the tree no longer holds is restated as the
  empty hash (as v1 snapshots already were) and named as drift. `rfc3339` removed. Pinned by
  `plan::tests::a_v2_header_naming_a_deleted_file_is_restated_as_the_empty_hash_not_refused` and
  `a_header_is_not_written_for_an_item_anchor_on_a_missing_file` (that caller refuses first);
  `plan_store/live.rs` guards with `is_file`. Tests: `tests/snapshot_writes_a_missing_header.rs`.
- Deleted: `LanguageBackend::anchor_for`, and in `backends/rust.rs` `anchor_for`, `anchor_opening`,
  `module_outline`, `OutlineItem`, `places_of`, `refuse_non_adjacent` (dead since #537).

Code issues closed, with their final measurement:
- `broken-restructure-anchors-empty-outline` — repo-scale run, 2026-10-21, `anchors
  packages/tddy-workflow/src/ids.rs --items GoalId`: cold in `./dev` 2 s exit 0, warm first and second request
  0 s exit 0, all three resolving `tddy_workflow::ids::GoalId`; cold outside `./dev` exits 1 with
  `language server not found: rust-analyzer: No such file or directory (os error 2)`. `places_of` is gone.
- `dead-code-plan-filehint-modified` — `grep -rn modified packages/tddy-code-restructuring/src/plan*`: no field,
  no writer, no reader.

Docs: [item-anchors.md](../item-anchors.md), [plan-store.md](../plan-store.md),
[readiness-and-gates.md](../readiness-and-gates.md).
