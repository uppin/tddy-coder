# 2026-10-25 — The daemon's start-up call of the headless sweep has no automated test

**Category:** Deferred from `#coagents` 9/9 (#776, sweep) — test gap
**Source:** `packages/tddy-daemon/src/runtime/mount_host.rs` (`BuildContext::build_session_host` → `sweep_headless_files`)

The daemon runtime calls `sweep_headless_files_left_by_this_daemon(config, tddy_data_dir)` in
`build_session_host`, before the host serves, and logs a failure instead of refusing to start. The
function it calls is covered
(`headless_tests::the_start_up_sweep_removes_what_this_daemon_planned_and_leaves_another_daemons_files`,
`tddy-coding-subagent/tests/sweep_headless_dirs.rs`), but nothing proves the runtime calls it, or
calls it before the first conversation can open: deleting the call leaves every test green.

Deferred because the runtime's `BuildContext` is not constructible in a unit test today, and building
a seam for one call was out of the sweep node's scope.

## What would close it

A daemon-level test that starts the runtime (or the narrowest constructible part of
`build_session_host`) on a data directory holding a `headless/<id>/` directory tagged with that
daemon's id and one tagged with another, and asserts after start-up that only the first is gone.
