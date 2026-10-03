# 2026-10-03 — `restructure` renders stale operations and shares the `--items` rule

**Type:** Feature

`#live-plan` 7/15, PR [#539](https://github.com/uppin/tddy-coder/pull/539). Cross-package entry:
[2026-10-03-index-daemon-live-plans.md](../../../../docs/dev/changesets/2026-10-03-index-daemon-live-plans.md).

`index_client.rs` / `index_console.rs` print the stale operations of `ListPlans`, `PlanStatus` and `Check`
through `console::stale_operations`, the renderer the in-process CLI uses, and read `--items` through
`item_anchor::parse_item_list`. `restructure snapshot` of an item-anchored plan stays in process (no
`Snapshot` RPC), so it starts its own language server even with `TDDY_INDEX_SOCKET` set.
