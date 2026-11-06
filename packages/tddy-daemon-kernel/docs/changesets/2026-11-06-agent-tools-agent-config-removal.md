# 2026-11-06 — The daemon.yaml agent types and readers are deleted

**Type:** Feature (breaking)

`#agent-tools` 19/19 ([#825](https://github.com/uppin/tddy-coder/pull/825)).

- `DaemonConfig.{claude_cli, cursor_cli, cli_harnesses}`, `ClaudeCliConfig`, `CursorCliConfig`, `CliHarnessConfig` and every reader (`resolve_claude_*`, `resolve_cursor_*`, `claude_cli_answer_wait_secs`, `claude_cli_renderer_env`, `cli_harness_binary`, `daemon_hook_urls::claude_hook_daemon_url`), `choose_*_binary`, `*BinaryChoice`, the `CLAUDE_*`/`CURSOR_*` env constants and `DEFAULT_ANSWER_WAIT_SECS` are deleted. `config/agent_cli.rs` keeps only `is_wrapper_shim_dir`, `resolve_harness_binary_name` and `find_real_harness_*`.
- `refuse_removed_agent_keys` stays; its `claude_home_dir` replacement now names `home: { kind: path, path: <dir> }`.
- Tests of the removed readers (`claude_cli_config_tests`, `cursor_cli_config_tests`, `tests/answer_wait_secs_config.rs`) are deleted.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-agent-config-removal.md).
