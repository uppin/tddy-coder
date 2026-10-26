# 2026-10-25 — DescribeSessionStartOptions

**Type:** Feature

`#start-parity` 5/9 ([#754](https://github.com/uppin/tddy-coder/pull/754)). New `start_options`:
`DESCRIBE_SESSION_START_OPTIONS`, `PR_STACK_RECIPES`, `StartOptionsQuery` (`deny_unknown_fields`),
`LabelledChoice`, `ModelChoices`, `ProviderRouteChoice`, `CandidateSession`, `StartOptions`, the
form's stack filters `pr_stack_orchestrators` / `stack_base_session_candidates`, `provider_harness`,
`models_agent` and `start_options_tool_def`. `project_home_tools`: the tool is a project-home target
tool (takes `daemon_instance_id`), with `ProjectHomePort::start_options` and its dispatch.

Tests: `tests/start_options.rs` (over `tddy-web/cypress/support/fixtures/stack-parent-candidates.json`).
