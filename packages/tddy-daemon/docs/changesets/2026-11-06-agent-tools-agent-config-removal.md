# 2026-11-06 — Fixtures and wiring follow the agent-config removal

**Type:** Feature

`#agent-tools` 19/19 ([#825](https://github.com/uppin/tddy-coder/pull/825)).

- `telegram_coding_agents(tddy_data_dir)` and `mount_host` call `host_coding_agents` without the config. `split_session_resume_acceptance`, `session_agent_remote_acceptance` and two more suites install their stubs through `coding_agents_with_binaries`; n18's `AFixtureHost` is replaced.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-agent-config-removal.md).
