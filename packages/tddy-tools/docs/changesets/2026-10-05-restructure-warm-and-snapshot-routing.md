# 2026-10-05 — `restructure warm`, and `snapshot` routed to the daemon

**Type:** Feature

Cross-package entry:
[2026-10-05-restructure-same-crate-moves.md](../../../../docs/dev/changesets/2026-10-05-restructure-same-crate-moves.md).
Product entry:
[2026-10-05-restructure-same-crate-moves.md](../../../../docs/ft/coder/changelog/2026-10-05-restructure-same-crate-moves.md).

- **`restructure warm`** (`index_client.rs`, rendered by `index_console.rs`) calls the daemon's `Warm` RPC
  for the tree's root and narrates its progress until the stream's final `ready` message, then says the
  root is warm. A stream that ends without `ready` is an error: reporting it as warm would send the next
  request to pay for the load the command was run to take off it. With `TDDY_INDEX_SOCKET` unset the
  command is refused in process, naming `./run-index-daemon` (`RestructureError::WarmNeedsIndexDaemon`),
  because a crate graph loaded by a process that exits is dropped with it.
- **`restructure snapshot` routing.** `answered_without_an_index` keeps a snapshot in the CLI process only
  when the plan has no item anchors (`item_anchor::plan_file_has_item_anchors`); a plan with item anchors
  goes to the daemon's `Snapshot` RPC when `TDDY_INDEX_SOCKET` is set, and the response is rendered by
  `console::snapshot_lines`, so it reads as the in-process run does. The `TODO(live-plans)` marker that
  named the missing RPC is gone. Without a daemon, an item-anchored snapshot still starts its own
  language server.

Tests (`tests/index_daemon_client_acceptance.rs` and unit tests in `index_client.rs`): a snapshot of an
item-anchored plan is answered by the daemon, one with no item anchors in process (a guard), `warm` with no
daemon refuses naming the script, and one `#[ignore]`d production test against a real rust-analyzer (the
daemon reports the root warm after `warm`).

Code issues: none of the four records of this package names `index_client.rs` or `index_console.rs`;
unchanged.
