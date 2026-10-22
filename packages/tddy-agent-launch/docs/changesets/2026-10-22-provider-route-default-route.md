# 2026-10-22 — A start that names no account reads the owner's default

**Type:** Feature

`#provider-route` 5/9 ([#666](https://github.com/uppin/tddy-coder/pull/666)).
`SessionAccountAccess::default_provider_account(harness)` and
`session_route_sources_with_default(req, harness, access)`, which fills `RouteSources.user_default`
only when the request names no account and no "built in". The unsandboxed claude-cli and opencode-cli
starts use it, and the sandboxed claude-cli start's refusal counts a default too. A locked vault
refuses a start that names nothing; an explicit choice never reads the vault for a default. Tool
sessions are not fed yet (`docs/dev/todo/2026-10-22-tool-session-starts-ignore-the-provider-default.md`).
See [module layout § Provider route of a session](../module-layout.md#provider-route-of-a-session).
