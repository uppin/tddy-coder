# 2026-10-21 — claude-cli start and resume apply a provider route

**Type:** Feature

`#provider-route` 2/9 ([#663](https://github.com/uppin/tddy-coder/pull/663)). New `session_provider_route` module: `session_route_sources`, `resolve_session_route`,
`start_route` / `StartRoute`, `pty_route_env`, `route_sources_on_resume`, `resume_route_env` and
`refuse_route_in_sandbox`. `SessionAccountAccess::provider_account` reads a provider account from the
owner's vault with the GitHub identity's refusals. `spawn_claude_cli_session_inner` takes
`&RouteSources`, resolves the route before creating anything, records `provider-account.yaml` and
appends the route env to the PTY; child and conversation spawns pass the default (vendor). Resume
assembles its agent env in `LaunchSessions::resumed_agent_env`, re-reading the key from the vault. A
sandboxed claude-cli start on an account is refused `FAILED_PRECONDITION`
(`TODO(provider-route-jail)`). See [module layout](../module-layout.md#provider-route-of-a-session).
