# 2026-11-06 — Binaries, tddy-tools and hook urls come from the spec on the daemon's host

**Type:** Feature

`#agent-tools` 19/19 ([#825](https://github.com/uppin/tddy-coder/pull/825)).

- `host_coding_agents::daemon_tool_host(base)`: the spec's `binary.env` on `base`, then `$HOME/.local/bin`, then `PATH` without shims; `tddy-tools` defaults to the daemon's sibling. A binary found nowhere is refused by the spec resolution. `host_coding_agents(tddy_data_dir)` takes no config; `no_coding_agents()` is the empty registry of a host that installed none.
- `LaunchSessions.coding_agents` is non-optional. `cli_harness_jail::agent_tddy_tools` gives a session's hooks, workflow and tool server the host-resolved `tddy-tools`; hook urls come from `listen.advertise_url`.
- A launch records its hook answer wait (`claude_cli_spawn_steps::record_hook_answer_wait`). The out-of-band `CLAUDE_CODE_NO_FLICKER` additions are gone. The jail home gains the `HomeSection::Path` arm.
- Deleted: `configured_cli_binary`, `CliHarnessStart.renderer_env`, `harness_binary`'s config step, n17's interim cursor lookup and its unit test `host_coding_agents_unit_tests.rs`.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-agent-config-removal.md).
