# 2026-11-06 — `BranchIntentPolicy`'s per-agent constructors are deleted

**Type:** Refactor

`#agent-tools` 17/19 ([#823](https://github.com/uppin/tddy-coder/pull/823)).

- `BranchIntentPolicy::{claude_cli, cursor_cli, cli_harness}` are deleted. Every CLI agent's branch intent is `BranchIntentPolicy::cli_agent(session_type, branch_name_required)`, built from the agent's spec by `tddy_agent_launch::harness_from_spec::cli_branch_intent`; the built-ins behave as before.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-cli-dispatch-collapse.md).
