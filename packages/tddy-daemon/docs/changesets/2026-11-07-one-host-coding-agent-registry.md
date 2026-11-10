# 2026-11-07 — The daemon loads its coding-agent registry once and refuses to start without it

**Type:** Fix

`#agent-tools` 22/28 ([#830](https://github.com/uppin/tddy-coder/pull/830)).

- `runtime::build` calls `host_coding_agents(tddy_data_dir)` once, before `BuildContext::new`, and `?`s the error: a daemon whose `<tddyhome>/tools` does not load stops, whatever is mounted. `BuildContext.coding_agents` holds the one `Arc<HostCodingAgents>`.
- Telegram (`build_telegram(…, coding_agents)`), the reserved-ids step (`mount_host`) and `build_session_host` (`DaemonSessionHost::new(…, coding_agents)`) read that instance. `telegram_coding_agents` and its log-and-empty branch are deleted (closes the `TODO(sweep)`).
- `RuntimeTasks::telegram_coding_agents()` exposes the registry Telegram's inbound control was built with; `tests/host_registry_load.rs` proves the sharing with `Arc::ptr_eq` and the refusal naming the file.
- `accounts_service_over` takes `Arc<CodingAgentRegistry>` (no `Option`).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-one-host-coding-agent-registry.md).
