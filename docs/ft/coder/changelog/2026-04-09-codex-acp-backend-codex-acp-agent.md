# 2026-04-09 — Codex ACP backend (`codex-acp` agent)

- **tddy-core**: **`CodexAcpBackend`** speaks ACP to a **`codex-acp`** subprocess (mirrors **`ClaudeAcpBackend`**); session resume via **`load_session`**; OAuth retry path reuses **`codex login`** + **`codex_oauth_authorize.url`** when ACP reports auth-like errors and **`session_dir`** is set; **`agent-client-protocol`** **`=0.10.4`** with **`unstable`**. **`AnyBackend::CodexAcp`**, backend menu / CLI mapping for **`codex-acp`**; **`task.rs`** treats **`codex-acp`** like **`codex`** for **`codex_thread_id`** persistence.
- **tddy-coder**: **`--agent codex-acp`**, **`create_backend`** wiring, **`TDDY_CODEX_ACP_CLI`** override alongside existing Codex CLI env for OAuth helper.
- **tddy-acp-stub** / **tddy-integration-tests**: protocol bump; stub **`initialize`** advertises **`load_session`**; **`codex_acp_backend`** acceptance tests.
- **Docs**: [codex-acp-backend.md](../codex-acp-backend.md); **[docs/dev/changesets/](../../../dev/changesets/)**; package **`changesets.md`** for **tddy-core** and **tddy-coder**.
