# 2026-10-05 — Restructure moves items and modules inside a crate, and the lifecycle moves it unblocked

**Type:** Feature

`feature/restructure/same-crate-moves`, one PR. Product entry:
[2026-10-05-restructure-same-crate-moves.md](../../ft/coder/changelog/2026-10-05-restructure-same-crate-moves.md).

`#carve` 16a deferred four steps because `tddy-tools restructure` had no same-crate move. This change adds
`move_item` (items into a module of the same crate, optionally creating it) and `reparent_module` (a module
and its directory under another parent), a `reexport: outside` facade mode for both, and three ergonomics
fixes found in the same run: the repo-root path named when `anchors` gets a package-relative one,
`restructure warm` (and `./run-index-daemon` warming its checkout), and a `Snapshot` RPC so an item-anchored
plan is snapshotted on the warm index. It then **performed the deferred steps with the new operations**; every
refusal or compile failure on the way was fixed in the engine, not worked around by hand.

| Package | Entry |
|---|---|
| `tddy-code-restructuring` | [same-crate-moves](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-05-same-crate-moves.md): the operations, `reexport: outside`, the repo-root hint, the engine defects the lifecycle moves found |
| `tddy-index-daemon` | [snapshot-rpc-and-warming](../../../packages/tddy-index-daemon/docs/changesets/2026-10-05-snapshot-rpc-and-warming.md): the `Snapshot` RPC and the script's warming |
| `tddy-tools` | [restructure-warm-and-snapshot-routing](../../../packages/tddy-tools/docs/changesets/2026-10-05-restructure-warm-and-snapshot-routing.md) |
| `tddy-session-lifecycle` | [same-crate-moves-lifecycle-layout](../../../packages/tddy-session-lifecycle/docs/changesets/2026-10-05-same-crate-moves-lifecycle-layout.md): eleven plans, no behaviour change |

Other packages were touched only where a test fake implements the generated `CodeIndexService` trait, which
gained a `snapshot` method: three test files of `tddy-daemon` and two of `tddy-lsp-executor`. The repo-root
script `run-index-daemon` gained the warming and `--no-warm`.

**Backlog resolved and deleted:**
`2026-10-04-restructure-extract-module-cannot-gather-items-from-several-files` (`move_item`, with `name`)
and `2026-10-04-restructure-anchors-and-snapshot-friction-seen-in-carve-16a` (the hint, `warm`, and the
`Snapshot` RPC; the "snapshot crash" it reported was the documented gap that an item-anchored snapshot had no
RPC, not an empty `files` header). **Narrowed:** `2026-09-24-lifecycle-modules-to-re-parent-by-hand` (three
rows remain, for node 17) and `2026-10-03-live-plans-three-gaps-in-staleness-reporting-and-snapshot-routing`
(its item 2). **Filed:** the six limits entries named in the engine package's entry.

**Code issues:** `broken-restructure-anchors-empty-outline` no longer names #537 as in flight (merged
2026-10-02); the remainder is unowned. The records of `tddy-code-restructuring` and
`tddy-session-lifecycle` were re-measured; none was closed (numbers in the package entries).

**Left open, with the developer's consent:** the 500-production-line budget. `backends/rust.rs` grew by 22
lines (2,831 to 2,853, wiring only), and `plan.rs` (481 to 520), `plan/codec.rs` (437 to 514) and
`item_anchor.rs` (463 to 517) crossed 500.

## Verification (on the tree rebased onto master `f014ee78`, 2026-10-05)

- `cargo check --all-targets` clean for `tddy-code-restructuring`, `tddy-tools`, `tddy-index-daemon`,
  `tddy-session-lifecycle`, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-lsp-executor`, `tddy-telegram-control`,
  `tddy-desktop` and `tddy-session-agents`.
- `tddy-code-restructuring`, `tddy-tools` and `tddy-index-daemon`, run together with `--no-fail-fast`:
  **1694 passed, 0 failed, 12 ignored** across 125 test binaries. The 12 ignored are the production tests
  that boot a real rust-analyzer (each of those the change added was run deliberately by hand and passed).
- `tddy-session-lifecycle`: **575 passed, 22 failed, 1 ignored**, the 22 failing tests identical to the
  baseline by name (the macOS-only sandbox and LiveKit suites), the same numbers as before the moves.
- `cargo clippy -D warnings --all-targets` and `cargo fmt --check` clean on the six touched packages.
- Three engine files crossed the 500-production-line budget (`item_anchor.rs` 463 to 517, `plan.rs` 481 to 520,
  `plan/codec.rs` 437 to 514); the developer deferred the split (backlog entry
  `2026-10-05-restructure-engine-files-past-the-500-line-budget`).

