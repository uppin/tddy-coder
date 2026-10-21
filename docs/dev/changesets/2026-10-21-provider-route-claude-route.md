# 2026-10-21 — Provider routes: claude-cli sessions run on a connected provider account

**Type:** Feature

`#provider-route` 2/9 ([#663](https://github.com/uppin/tddy-coder/pull/663)), across `tddy-agent-backend` (the `provider_route` module: harness ids,
the capability matrix for all ten harnesses, Claude Code route bodies, precedence),
`tddy-service` (`StartSessionRequest.provider_account_id = 41` / `provider_built_in = 42`;
`accounts.ListProviderRoutes`), `tddy-accounts` (`ListProviderRoutes` behind the `RouteSupport`
port), `tddy-daemon` (`HarnessRouteSupport` wiring), `tddy-changeset` (`provider-account.yaml`),
`tddy-agent-launch` (vault read, claude-cli start and resume, sandbox refusal) and `tddy-web`
(`CreateSessionProviderField`). `ListProviderRoutes` lives on `AccountsService`, not
`CatalogService`, because the answer needs the vault; the per-session record sits beside
`.session.yaml` rather than in it. Only the Claude harnesses have route bodies; every other routable
harness answers `NotYetRoutable` until its node. A sandboxed claude-cli start on an account is
refused (`TODO(provider-route-jail)`, `#provider-route` 9/9).

Docs: [Claude Code CLI session](../../ft/daemon/claude-cli-session.md#running-on-a-provider-account),
[provider accounts](../../ft/daemon/provider-accounts.md#running-a-harness-on-an-account),
[tool-session model selection](../../ft/web/tool-session-model-selection.md#provider),
[tddy-agent-backend](../../../packages/tddy-agent-backend/docs/architecture.md#provider-routes-provider_route),
[tddy-agent-launch](../../../packages/tddy-agent-launch/docs/module-layout.md#provider-route-of-a-session).

Code issues: `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md`
and `packages/tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md` were touched
alongside, not fixed — the route is resolved in helpers outside `start_session_core`, and the
resolver is its own module, so `backend/mod.rs` is not grown. Both records stay as they are.
`packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-claude-cli-session-resume-claude-cli-session.md`
is left unchanged in length by this PR: the resume's route environment is assembled in
`LaunchSessions::resumed_agent_env`, not in `resume_claude_cli_session`'s body.
`packages/tddy-accounts/src/service.rs` grew 647 → 712 production lines (`list_provider_routes`,
`with_route_support`); its file-budget entry stays deferred for the stack-shared reason it records.

`spawn_claude_cli_session_inner` (`tddy-agent-launch/src/claude_cli_spawn.rs`) measures complexity
13, CRAP 182 (never executed) on `tddy-tools analyze`: the route resolution and the session
directory it records into are `create_routed_session_dir`, which its own unit tests cover. The
function is 281 → 288 lines (the route-sources parameter, the call and the route env). Routability
has one source, `provider_route::route_body`, read by both `resolve_route` and `harness_routable`,
so `ListProviderRoutes` offers no account for a harness whose route is not built.
