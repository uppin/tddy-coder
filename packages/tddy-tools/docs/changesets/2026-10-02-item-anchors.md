# 2026-10-02 — `restructure anchors --at` through the warm daemon

**Type:** Feature

`#live-plan` 1/7, PR [#537](https://github.com/uppin/tddy-coder/pull/537). Cross-package entry:
[2026-10-02-item-anchors.md](../../../../docs/dev/changesets/2026-10-02-item-anchors.md).

`index_client.rs` carries `--at L:C[-L:C]` into `AnchorsRequest.at`, and `index_console.rs` prints the
anchor the daemon returns (`anchor_json`) in the form a plan line carries. `tddy-tools restructure
anchors` therefore answers `--items` and `--at` the same way against the warm daemon as in process.
Documented in [rust-code-restructuring.md](../../../../docs/ft/coder/rust-code-restructuring.md#item-anchors).

Code issues: the four records here name `cli.rs` and `server.rs` functions this change did not touch;
none was re-measured or changed. 1137 passed, 0 failed, 9 ignored across the four touched packages.
