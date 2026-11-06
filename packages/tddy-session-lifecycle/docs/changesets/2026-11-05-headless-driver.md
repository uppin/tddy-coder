# 2026-11-05 — Every headless conversation is planned from its spec

**Type:** Refactor

`#agent-tools` 15/19 ([#821](https://github.com/uppin/tddy-coder/pull/821)).

- `headless_plan` plans `claude`, `cursor`, `codex` and `oz` only through `headless_plan_from_spec`; a daemon without its coding agents refuses. `headless_program`, `mcp_servers_by_name`, `write_cursor_project_mcp_config` and `OZ_BINARY` are deleted; the worktree file's git-exclusion is `tddy_agent_backend::turn_files::exclude_from_git`.
- Tests: the `headless_tests.rs` cases that pinned hand-built files and `codex_and_oz_run_by_name_from_the_path` are deleted (approved); the spec-planned conversations are covered by `tests/headless_claude_subagent_from_spec_acceptance.rs` and `tests/headless_subagents_from_spec_acceptance.rs`.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md).
