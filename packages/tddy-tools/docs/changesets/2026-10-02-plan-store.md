# 2026-10-02 — `restructure load`, `unload`, `plans`

**Type:** Feature

`#live-plan` 2/7, PR [#538](https://github.com/uppin/tddy-coder/pull/538). Cross-package entry:
[2026-10-02-plan-store.md](../../../../docs/dev/changesets/2026-10-02-plan-store.md).

`index_client.rs` routes `restructure load`, `unload` and `plans` to the daemon's `LoadPlans`,
`UnloadPlans` and `ListPlans`, and prints what the store holds (`index_console.rs`). Without
`TDDY_INDEX_SOCKET` the three are refused as needing a daemon. `--from <id>` through the daemon is
refused with an error naming the workaround (`TODO(plan-store)` in `from_index`: `ApplyRequest` has no
field for it).

Test: `restructure_load_without_a_daemon_is_refused_as_needing_one`.
