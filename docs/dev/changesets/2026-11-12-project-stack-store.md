# 2026-11-12 — PR stacks live in the project, not in a session

**Type:** Architecture

`#orchestrators 2/11`, PR [#879](https://github.com/uppin/tddy-coder/pull/879), stack
[#889](https://github.com/uppin/tddy-coder/stack/889). Feature:
[pr-stacking.md § Where a stack lives](../../ft/coder/pr-stacking.md#where-a-stack-lives).

## What changed

- **`tddy-changeset`** — `project_stack_store`: `ProjectStackStore { create, read, update, list }`,
  `StackRecord`, `StackStoreError`, records at `<projects dir>/<project_id>/stacks/<stack_id>.yaml`
  (UUIDv7 ids) under a per-stack in-process lock with an atomic rename. `Changeset.stack_id`.
  `read_session_stack` / `read_stack_of` read the record a session names; `update_stack_atomic`
  writes it and creates it on a session's first write (record first, then id). Both refuse a
  changeset still carrying the pre-store `Changeset.stack`. `stack_fixtures` (feature `test-support`)
  for other crates' tests. Adds `uuid` (`v7`, already in the workspace) and an optional `tempfile`.
- **`tddy-pr-stack`** — `stack_migration::migrate_session_stacks`, the startup sweep: the only reader
  of `Changeset.stack`. `stack_ops` and `assess` read and write through the store.
- **`tddy-daemon`** — runs the sweep in `build_session_host`, before the host serves, over its data
  directory.
- **`tddy-workflow-recipes`, `tddy-tools`, `tddy-daemon-rpc`, `tddy-agent-launch`,
  `tddy-session-worktree`, `tddy-session-activity`** — call sites swapped to the store. The session
  list's `stack_plan_json` is read from the store; an unmigrated session lists none.
- **Removed**: `PrStackRecipe::next_goal_for_state_with_changeset` — the `"Init"` → `orchestrate`
  resume rule keyed on `Changeset.stack`. The pr-stack orchestration loop is deprecated, so the rule
  was deleted with the field, not re-keyed on `stack_id`.

## Decisions

- **No dual read** (developer, 2026-11-12): only the sweep reads `Changeset.stack`. Approved test
  change: `stack_plan_json_from_store::the_session_list_reports_the_same_stack_plan_json_before_and_after_migration`
  became `a_migrated_sessions_list_reports_the_stack_plan_it_carried_before_migration` plus
  `an_unmigrated_session_lists_no_stack_plan`.
- **Store in `tddy-changeset`**, beside `Stack`: every caller already depends on it, so no crate edge
  is added. One dev-dependency edge: `tddy-session-activity` → `tddy-pr-stack` (its acceptance test
  runs the sweep).
- **The sweep is not per OS user, by design.** Every daemon resolver passes the daemon's data
  directory to `sessions_base_for_user` as its `config_dir`, which wins over the per-user
  `$HOME/.tddy` default, so the daemon serves exactly one sessions base. Sweeping
  `<tddy_data_dir>` covers every session it can serve.
- **Older daemons cannot read a migrated stack.** No downgrade path.

## Wrap fixes

- `update_stack_atomic` takes the process-wide first-write lock only on a session's first write
  (re-reading under it); an existing stack takes only its per-stack lock. The session id comes from
  the validated session directory instead of an `unwrap_or_default`.
- Unused `session_stack_id` and the public `session_stack_store` dropped (now a private
  `session_stack_home`).
- The sweep logs its own summary; the daemon calls it through one `sweep_what_the_last_run_left`
  helper beside the headless-files sweep, instead of re-logging each failure.
- Daemon-level acceptance test `startup_stack_migration_acceptance`: `runtime::build` over a legacy
  data dir leaves it migrated.
- `pr_stack/ports.rs` re-reads `stack_plan_json` through one `stack_plan_json_of` helper:
  819 → 818 production lines (open record
  [oversized-file-pr-stack-ports](../../../packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md)).
- `runtime/mount_host.rs`: 489 → 499 production lines (under the 500 budget);
  `build_session_host` 74 → 73 lines (open record
  [complexity-mount-host-build-session-host](../../../packages/tddy-daemon/docs/code-issues/complexity-mount-host-build-session-host.md)).
