# 2026-10-02 — Item anchors

**Type:** Feature

`#live-plan` 1/7, PR [#537](https://github.com/uppin/tddy-coder/pull/537). Cross-package entry:
[2026-10-02-item-anchors.md](../../../../docs/dev/changesets/2026-10-02-item-anchors.md). Product
entry: [2026-10-02-item-anchors.md](../../../../docs/ft/coder/changelog/2026-10-02-item-anchors.md).

`Anchor::Item` and `Anchor::Items` join `Symbol` and `Range`; `ItemPath`, `Fingerprint`, `FileHint`
and the v2 header live in `plan.rs`. `item_anchor.rs` holds the shared half (module path of a file,
relative to absolute ranges, the fingerprint check, `resolve_item_anchors`, `ItemResolver`);
`backends/rust/item_path.rs` walks the outline. `LanguageBackend::item_resolver` defaults to `None` and
the registry answers `UnsupportedOp { op: "item anchors" }`; a file no backend claims is `NoBackend`.

`open_run_resolving_anchors` is the one order both apply loops use: item anchors resolve, then the
baseline compile gate, then `.restructure/` is written. `RestructureError::ItemChanged { item, file }`
names the item and its file, not the operation. `ItemAnchorsOnContinuedRun` refuses a continued run
(`TODO(plan-store)` in `runner/entry_points.rs`). A static `check` reports each item-anchored
operation as a finding. `anchors` gains `--at` and returns an anchor value (`Outcome::ItemAnchored`).

`settled_outline` believes an empty outline only when it has items, the index is loaded, or the server
was observed quiescent, and refuses a degraded index; before, it waited forever on a file that defines
nothing. See [item-anchors.md](../item-anchors.md).

Tests: `tests/item_anchor_acceptance.rs` (12), `tests/anchors_command_acceptance.rs` (2), unit tests in
`plan.rs`, `item_anchor.rs`, `backends/rust/item_path.rs` and `restructure_args.rs`.
`./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools -p tddy-lsp`: 1137 passed,
0 failed, 9 ignored. clippy and fmt clean on those packages.

## Code issues, final measurements

| Record | Before | After | Disposition |
|---|---|---|---|
| `oversized-file-plan` | 433 production lines | **799** | Created, open. Over budget in this PR; split deferred because later `#live-plan` nodes extend the file |
| `oversized-file-runner-entry-points` | 506 | **602** | Created, open. Same reason |
| `oversized-file-backends-rust` | 4,359 | **4,433** | Kept, regressed +74 |
| `broken-restructure-anchors-empty-outline` | claimed by #537 | claim kept | **Narrowed, not closed.** The empty-outline wait is reproduced and fixed; the warm-path refusal is inferred, not reproduced, and the cold path's `lsp server exited` is unaddressed. Closing it needs `anchors <file> --items A,B` run at repo scale, cold and warm. Deferred by the developer on 2026-10-02 |

New record `dead-code-plan-filehint-modified` (`FileHint.modified` is written into the v2 header and read
by nothing; 0 read sites) is open. The deferred defects that fit no record category are in the backlog,
named in the cross-package entry.
