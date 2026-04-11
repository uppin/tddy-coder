# Codex ACP backend (`--agent codex-acp`)

**Product area:** Coder  
**Status:** Current  

## Summary

**tddy-coder** supports **`--agent codex-acp`**: a **`CodingBackend`** that speaks the **Agent Client Protocol (ACP)** to the **`codex-acp`** stdio agent (subprocess), alongside the existing **`--agent codex`** path that runs **`codex exec`** JSONL. Session resume uses ACP **`load_session`** with the identifier stored in the session’s **`codex_thread_id`** file—the same on-disk field **`codex`** uses—so workflow code treats **`codex`** and **`codex-acp`** consistently.

## CLI and configuration

- **`--agent codex-acp`** selects **`CodexAcpBackend`**.
- **`codex-acp` executable:** resolved like other CLIs—**`TDDY_CODEX_ACP_CLI`**, optional config **`codex_acp_cli_path`**, optional **`--codex-acp-cli-path`**, else the **`codex-acp`** name on **`PATH`** (constant **`DEFAULT_CODEX_ACP_BINARY`**).
- **OAuth helper:** uses the same **`codex`** binary resolution as **`CodexBackend`** (**`TDDY_CODEX_CLI`**, **`--codex-cli-path`**, config, then **`PATH`**) to run **`codex login`** when ACP **`new_session`** / **`load_session`** fails with an auth-like error and **`session_dir`** is set. That reuses the **`BROWSER`** / **`codex_oauth_authorize.url`** contract documented for headless OAuth (see [Codex OAuth web relay](../web/codex-oauth-web-relay.md)).

## Behavior

- **Worker model:** dedicated thread, **`tokio::LocalSet`**, **`ClientSideConnection`** over the child’s stdio (**`agent-client-protocol`** **0.10.4**, **`unstable`** feature for request builders).
- **Lifecycle:** **`initialize`** once, then **`new_session`** or **`load_session`** (resume), then **`prompt`** with merged Codex-style prompts (**`merge_codex_prompt`**).
- **Progress:** **`session_notification`** maps agent chunks, tool calls, and plan entries to **`ProgressSink`** (**`TaskProgress`**, **`ToolUse`**, **`TaskStarted`**).
- **Permissions:** **`request_permission`** auto-approves with a one-shot allow option when enabled.
- **Tests:** **`tddy-acp-stub`** can stand in for **`codex-acp`**; stub **`initialize`** advertises **`load_session`**. Integration coverage includes **`codex_acp_backend`** and **`acp_backend_acceptance`**.

## Coexistence with `CodexBackend`

**`--agent codex`** remains the JSONL **`codex exec`** integration. Operators choose per session whether to use ACP or JSONL; both backends share **`codex_thread_id`** persistence rules in **`workflow/task.rs`**.

## Related documentation

- [Coder overview](1-OVERVIEW.md) — backend selection
- [Implementation step](implementation-step.md) — goals and **`--agent`**
- [Session layout](session-layout.md) — session directory; **`codex_thread_id`**
- [Codex OAuth web relay](../web/codex-oauth-web-relay.md) — dashboard and **`codex_oauth_authorize.url`**
- [Codex OAuth relay (daemon)](../daemon/codex-oauth-relay.md) — validation helpers
- Implementation: **`packages/tddy-core/src/backend/codex_acp.rs`**
- [Agent Client Protocol](https://agentclientprotocol.com)
- [codex-acp](https://github.com/zed-industries/codex-acp) (stdio agent distribution)
