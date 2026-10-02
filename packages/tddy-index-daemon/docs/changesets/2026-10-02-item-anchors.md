# 2026-10-02 — The `Anchors` RPC carries item anchors

**Type:** Feature

`#live-plan` 1/7, PR [#537](https://github.com/uppin/tddy-coder/pull/537). Cross-package entry:
[2026-10-02-item-anchors.md](../../../../docs/dev/changesets/2026-10-02-item-anchors.md).

`AnchorsRequest` gains `optional SourceRange at`; `AnchorsResponse` gains `string anchor_json`, the
anchor a plan carries (`items` for named items, `item` for `at`), beside the absolute `range`. `queries.rs`
resolves through `tddy_code_restructuring::runner::item_anchors` on the warm server with a cancellation
token that fires when the request is dropped, so an empty outline does not outlive its caller. `cli.rs`
carries `--at` into the request; `render.rs` and `single_shot.rs` print the anchor. `status.rs` maps
`ItemChanged` and `ItemAnchorsOnContinuedRun` to `FailedPrecondition`.

`apply.rs` opens a run with `open_run_resolving_anchors`, the order the library's own apply loop uses:
item anchors resolve, then the baseline compile gate, then `.restructure/` is written. The
`code_index_service_acceptance` anchors test's workspace now writes a `Cargo.toml`, since an item path
is rooted in a package; its assertion is unchanged.

Tests: `cli.rs` `anchors_at_carries_the_position_rather_than_items`; the fake-server anchors tests
assert the `range` and the `anchor_json` for `--items` and for `--at`. 1137 passed, 0 failed, 9 ignored
across the four touched packages.

Code issues: `stale-repo-scoped-restructure-state-apply` re-measured; the call site is still
`apply.rs:45`, unchanged. The other two records in this package name code this change did not touch.
