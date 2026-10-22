# 2026-10-22 — OpenCode ACP backend and the shared ACP update handling

**Type:** Feature

`#provider-route` 7/9 ([#668](https://github.com/uppin/tddy-coder/pull/668)). `OpenCodeAcpBackend`
(`backend/opencode_acp.rs`, re-exported): `opencode acp` over ACP behind `AnyBackend::OpenCode`, a lazy
worker like the other ACP backends; `with_provider_route` writes the route's files owner-only
(`RouteFiles`, bare file names only) and names the first with `OPENCODE_CONFIG`, without the route's
args. Registered in `backend_selection_question` (seventh, `OpenCode`), `backend_from_label`,
`default_model_for_agent` / `curated_models_for_agent` (`OPENCODE_DEFAULT_MODEL` =
`fireworks/kimi-k2`), `preselected_index_for_agent` (6) and `model_catalog::build_backend`
(`BackendCliPaths.opencode_cli_path`). New `backend/acp_updates.rs` (`record_session_update`,
`allow_once`), now used by `acp.rs`, `codex_acp.rs` and `opencode_acp.rs` in place of three copies of
the client's notification handling. Code issues: `oversized-file-codex-acp` **closed** — 548 → 480
production lines (`acp.rs` 450 → 382); `oversized-file-backend-mod` 882 → 906 (stack-shared, split
deferred). See [architecture](../architecture.md#backend-backend).
