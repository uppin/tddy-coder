# 2026-11-04 — One coding-agent registry

**Type:** Feature (breaking)

`#agent-tools` 12/19 ([#801](https://github.com/uppin/tddy-coder/pull/801)).

- `DaemonConfig::load` refuses `claude_cli:`, `cursor_cli:` and `cli_harnesses:` (`refuse_removed_agent_keys`, `RemovedAgentConfigError`), naming `<tddyhome>/tools/<id>.yaml` and each field's new home.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-04-agent-tools-agent-registry.md).
