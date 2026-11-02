# 2026-11-02 — `DaemonSessionHost::with_coding_agents`

**Type:** Feature

`#agent-tools` 2/12 ([#791](https://github.com/uppin/tddy-coder/pull/791)). The builder installs the host's coding agents and carries them to `LaunchSessions::coding_agents` (`svc_coding_agents_wiring.rs`). New suite `tests/claude_cli_from_spec_acceptance.rs` with goldens under `tests/fixtures/claude_cli_from_spec/`. `oversized-file-svc-host-builders`: 503 → 504 (stack-shared, split deferred). See the [cross-package entry](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-host.md).
