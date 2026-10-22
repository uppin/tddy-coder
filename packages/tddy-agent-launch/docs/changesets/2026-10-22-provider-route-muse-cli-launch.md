# 2026-10-22 — CLI harness start and resume through the registry

**Type:** Feature

`#provider-route` 6/9 ([#667](https://github.com/uppin/tddy-coder/pull/667)). `start_session_core`
and `resume_session_at_session_coordinate` look a session type up in the CLI harness registry after
their claude-cli and cursor-cli branches, so `opencode-cli` and `muse-cli` share one path:
`LaunchSessions::start_cli_harness` (`cli_branch_starts.rs`; the seeded roster now resolved inside
`start_cli_harness_from_request`, which takes five arguments) and `start_cli_harness_session(CliHarnessStart)`
/ `LaunchSessions::resume_cli_harness` (`opencode_cli_spawn.rs`; the resumed session is one
`ResumedSession` argument). Neither keeps a `#[allow(clippy::too_many_arguments)]`.
`cli_route_launch::launch_model_for` blanks the launch's own model only for a routed harness with a
`route_config_env`, so a Muse route keeps the session's `--model`. Code issues re-measured, both
**unchanged** (same line count, by brace matching against `origin/feature/provider-route/default-route`):
`complexity-svc-start-session-core-start-session-core`, `complexity-svc-resume-session-resume-session-at-session-coordinate`.
See [module layout](../module-layout.md).
