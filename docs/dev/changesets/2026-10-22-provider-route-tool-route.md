# 2026-10-22 — Tool sessions with Claude and Codex agents on a provider account

**Type:** Feature

`#provider-route` 4/9 ([#665](https://github.com/uppin/tddy-coder/pull/665)), across
`tddy-agent-backend` (the Codex arm of `resolve_route` in `provider_route/codex.rs`; the
`provider_route_file` format and owner-only loader; `backend::routed` — `with_provider_route` on
`AnyBackend` and the four Claude/Codex backends, the ACP workers started on first use),
`tddy-spawn` (`write_provider_route_file`, `SpawnOptions.provider_route_json`,
`--provider-route-path`), `tddy-agent-launch` (`tool_session_route_json` at start and resume),
`tddy-coder` (`--provider-route-path`, `provider_route_arg.rs`; `create_backend` takes the route),
`tddy-telegram-control` (passes no route) and `tddy-web` (the Provider field for tool sessions). The
route travels in `<session_dir>/provider-route.json` (0600, chowned), never in argv or the spawn
environment. Cursor, the stub, a specialized definition and an account without an agent are
refused. The Codex `-c` overrides follow Codex's `config.toml` keys and were checked only against
`codex-acp --help`; every test runs a stub binary.

Restructuring at wrap (production lines, to the first `#[cfg(test)]`): `backend/claude.rs` 785 →
644 (transcript usage readers to `claude/transcript_usage.rs`), `backend/codex.rs` 520 → 479
(login text and thread-id file to `codex/login_text.rs`), `backend/codex_acp.rs` 604 → 541 (OAuth
relay to `codex_acp/oauth.rs`; `codex_acp_route_args` to `routed.rs`), `provider_route.rs` 519 → 452
(Codex arm to `provider_route/codex.rs`), `tddy-coder/src/run.rs` 2689 → 2595 (Codex binary
resolution to `codex_binaries.rs`, route loading to `provider_route_arg.rs`), `tddy-spawn
spawn_worker.rs` 507 → 481 (`CloneRequest` to `spawn_worker/clone_request.rs`). Paths are kept by
`pub use`. Not split, stack-shared: `backend/mod.rs` 879 → 880 (#668), `CreateSessionPane.tsx` 822 →
824 (#666).

Backlog: **added** `2026-10-22-tool-session-provider-picker-needs-harness-metadata` and
`2026-10-22-telegram-tool-sessions-choose-no-provider-account`.

Code issues: **narrowed** `oversized-file-claude` (767 → 644), `oversized-file-codex-acp` (562 → 541),
`oversized-file-run` (2655 → 2595). **Regressed, kept, stack-shared**: `oversized-file-backend-mod`
(879 → 880), `oversized-file-create-session-pane` (822 → 824). **Touched**:
`complexity-claude-invoke-sync`, `complexity-acp-run-acp-worker`,
`complexity-codex-acp-run-codex-acp-worker`.

Docs: [provider accounts § Tool sessions](../../ft/daemon/provider-accounts.md#tool-sessions),
[tool-session model selection](../../ft/web/tool-session-model-selection.md#provider),
[Codex ACP backend](../../ft/coder/codex-acp-backend.md),
[tddy-agent-backend](../../../packages/tddy-agent-backend/docs/architecture.md#routed-backends-provider_route_file-backendrouted),
[tddy-agent-launch](../../../packages/tddy-agent-launch/docs/module-layout.md#provider-route-of-a-session).
