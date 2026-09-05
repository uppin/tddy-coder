# 2026-04-09 — Codex ACP backend

**Type:** Feature

**`CodexAcpBackend`** (**`backend/codex_acp.rs`**, ACP **`ClientSideConnection`** to **`codex-acp`** subprocess), **`load_session`** on resume, OAuth retry via **`codex login`** + **`codex_oauth_authorize.url`**; **`ClaudeAcpBackend`** / shared ACP paths use **`InitializeRequest::new`**, **`NewSessionRequest::new`**, **`PromptRequest::new`** for **`agent-client-protocol` 0.10.4**; **`AnyBackend::CodexAcp`**; **`task.rs`** **`codex-acp`** **`codex_thread_id`** parity with **`codex`**. Feature doc: [codex-acp-backend.md](../../../../../docs/ft/coder/codex-acp-backend.md). (tddy-core, tddy-coder, tddy-acp-stub, tddy-integration-tests)
