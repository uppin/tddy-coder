# 2026-10-07 — `snapshot` writes the header a headerless plan lacks

**Type:** Feature

`plan/codec/headerless.rs` (new) holds the tolerant read and the header: `Plan::starts_with_an_operation`
(a first line carrying an `op` key and none of `v`/`snapshot`/`files`), `Plan::parse_headerless` (every
non-blank line is an operation, so every operation refusal surfaces as it would from `check`),
`Plan::anchored_files` (the primary and `also` anchors' files, sorted and de-duplicated) and
`Plan::header_for_anchored_files`. The version follows Decision O1: **v2** when every anchor of every
operation is `item`/`items`, **v1** when any is `range`/`symbol`; the line is serialised through
`hint_of` / `apply::hash_file` exactly as `rehashed_header` serialises a headed plan, so a second
`snapshot` rewrites nothing. `anchored_file_path` validates each file (empty, absolute or `..`, not a
regular file) before anything is written.

`runner/entry_points/check_entry_points.rs` routes a plan whose first line is an operation to
`insert_a_header`, which splices the header above the first non-blank line and carries every operation
byte for byte; `snapshot_resolving` routes through `plan_file_has_item_anchors`. `plan/codec.rs`'s
first-line refusal names `restructure snapshot` when the first line is an operation.

`tests/snapshot_writes_a_missing_header.rs` (new) holds the ten library tests.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-07-plan-header.md).
