# 2026-10-22 — Codex route and routed backends

**Type:** Feature

`#provider-route` 4/9 ([#665](https://github.com/uppin/tddy-coder/pull/665)). `resolve_route` routes
`Codex` / `CodexAcp` (`provider_route/codex.rs`: `-c model_provider…` overrides, `-m <model>`, secret
`TDDY_ROUTE_API_KEY`; `CODEX_ROUTE_PROVIDER_ID`, `CODEX_ROUTE_KEY_ENV`). New `provider_route_file`
(`PROVIDER_ROUTE_FILENAME`, `route_to_json`, `route_from_json`, `load_route_file` refusing any group or
other bit) and `backend::routed` (`with_provider_route` on `AnyBackend` — Cursor and the stub refused —
and on `ClaudeCodeBackend`, `ClaudeAcpBackend`, `CodexBackend`, `CodexAcpBackend`). The ACP backends
start their worker on first use. Split at wrap: `claude/transcript_usage.rs`, `codex/login_text.rs`,
`codex_acp/oauth.rs`, all re-exported. See
[architecture](../architecture.md#routed-backends-provider_route_file-backendrouted).
