# 2026-11-06 — Every launcher holds coding agents; fixtures install stubs through binary.env

**Type:** Feature

`#agent-tools` 19/19 ([#825](https://github.com/uppin/tddy-coder/pull/825)).

- `DaemonCodingAgentLauncher`'s agents are non-optional and its agentless hand-built ACP path is deleted (`harness_command`, the hand-built `apply_route` branch, `claude_agent_acp_command`, the ACP constants). `DaemonSessionHost::launch_coding_agents` is public (the catalogue reads it).
- `svc_terminal_ports.rs`: an agent tab's binary is its spec's, resolved through the host's coding agents. `svc_activity_ports.rs` no longer passes a daemon-wide wait.
- `test_util::coding_agents_with_binaries(&[(id, binary)])` replaces `daemon_config_from_fixture_file` and `builtin_coding_agents_for`; ~40 fixture files migrated. `tddy-actions` is a normal dependency. `start_session_binary_resolution_tests.rs` and `resume_session_binary_resolution_tests.rs` are deleted with the functions they tested.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-agent-config-removal.md).
