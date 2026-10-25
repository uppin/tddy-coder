# 2026-10-25 — Two hostname-named daemons sharing a data dir sweep each other's headless files

**Category:** Deferred from `#coagents` 9/9 (#776, sweep)
**Source:** `packages/tddy-coding-subagent/src/launcher.rs` (`sweep_headless_dirs`); `packages/tddy-session-lifecycle/src/coding_agent_launcher.rs` (`sweep_headless_files_left_by_this_daemon`, `HeadlessWiring.owner`)

Each `headless/<conversation>/` directory is tagged with the creating daemon's durable instance id
(`local_base_instance_id_for_config`: the configured `daemon_instance_id`, else the **hostname**),
and a daemon's start-up sweep removes the directories tagged with its own id. Two daemons on one
machine that share a data directory (Tddy Desktop beside a dev daemon) and **both** leave
`daemon_instance_id` unset get the same id — the hostname — so a daemon that starts while the other
holds open headless conversations deletes their MCP configs. A running `claude` or `oz` turn then
loses its tool server on its next invocation.

Deferred because the fix is a choice the developer has not made: refuse a shared data directory
without distinct ids, derive a durable per-install id (persisted on first start), or lock the data
directory per daemon.

## What would close it

Make the tag unique per daemon even when no id is configured — for example a durable id generated
once and persisted under the daemon's own state — and add a test with two daemons sharing a data
directory, neither configured with an id, in which one's start-up sweep leaves the other's open
conversation's files in place.
