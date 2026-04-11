# PRD: Migrate Codex Backend to ACP (codex-acp)

## Summary

Replace the current **`CodexBackend`** (`codex.rs`) — which spawns `codex exec … --json` and parses JSONL — with a new **`CodexAcpBackend`** that speaks **Agent Client Protocol (ACP)** to the **`codex-acp`** runtime embedded as a Rust library dependency. Authentication uses ACP's native `authenticate` flow with the OAuth URL surfaced to **tddy-web** via LiveKit participant metadata.

## Background and Motivation

The current Codex integration shells out to the `codex` CLI binary, parsing its JSONL stdout. This approach:

- **Fragile argv ordering:** Resume, sandbox, model flags have underdocumented positional constraints.
- **Binary distribution burden:** Requires a separate `codex` binary on `PATH`.
- **Limited protocol:** JSONL is one-way; no structured tool events, progress, or session management beyond thread-id scraping.
- **OAuth workaround:** Uses `BROWSER` env + binary re-exec hook to capture OAuth URLs — indirect and error-prone.

The **Agent Client Protocol** (ACP) provides bidirectional JSON-RPC over stdio with structured session management, tool events, progress notifications, and authentication methods. The **`codex-acp`** crate implements the ACP agent side for OpenAI Codex. Embedding it as a library removes the external binary dependency and enables direct Rust integration.

## Affected Features

- **[Codex OAuth web relay](../web/codex-oauth-web-relay.md)** — OAuth URL delivery mechanism changes from file-based `BROWSER` hook to ACP `authenticate` protocol.
- **[Codex OAuth relay (daemon)](../daemon/codex-oauth-relay.md)** — Validation helpers remain; capture source changes.
- **[Implementation step](implementation-step.md)** — Backend invocation path changes.
- **[Session layout](session-layout.md)** — `codex_thread_id` file semantics may change to ACP session id.

## Proposed Changes

### What changes

1. **New backend**: `CodexAcpBackend` in `packages/tddy-core/src/backend/codex_acp.rs`, following the `ClaudeAcpBackend` worker-thread + LocalSet + stdio pattern but embedding `codex-acp` as a library crate.
2. **AnyBackend variant**: New `CodexAcp` variant alongside existing `Codex`.
3. **CLI flag**: `--agent codex-acp` selects the new backend; `--agent codex` continues to use the old JSONL backend during transition.
4. **Authentication**: ACP `authenticate` flow surfaces OAuth URL; new `TddyCodexAcpClient` writes URL to `codex_oauth_authorize.url` (same file contract) so `participant.rs` poller and `ParticipantList.tsx` work without changes.
5. **Session resume**: ACP session id replaces Codex thread id; `workflow/task.rs` updated to handle both id types.
6. **Conversation log**: ACP raw messages logged to `conversation_output_path` (equivalent to JSONL append).

### What stays the same

- `participant.rs` file poller and JSON metadata shape (`codex_oauth.pending + authorize_url`).
- `ParticipantList.tsx` consumer — no web changes needed.
- Old `CodexBackend` kept until parity verified.
- `codex_oauth_relay.rs` validation helpers.
- Session directory structure.

## Technical Approach

- Embed `codex-acp` as a workspace dependency (git dep pinned to a specific tag).
- Mirror `ClaudeAcpBackend` architecture: dedicated thread, `tokio::LocalSet`, `ClientSideConnection` over stdio-compatible channels.
- Implement `Client` trait with `TddyCodexAcpClient` that handles `authenticate` (extract URL → write to file), `session_notification` (accumulate text + progress), and `request_permission` (auto-approve).
- Map `InvokeRequest` → ACP `NewSessionRequest` / `PromptRequest` with sandbox mode, model, working directory.

## Acceptance Criteria

1. `CodexAcpBackend` implements `CodingBackend` trait with `invoke` and `name` methods.
2. `--agent codex-acp` CLI flag creates and uses the new backend.
3. ACP session lifecycle: `initialize` → `new_session` → `prompt` → accumulate response.
4. Session resume works via ACP session id (persisted to `codex_thread_id` file or successor).
5. OAuth URL from ACP `authenticate` is written to `codex_oauth_authorize.url` and published to LiveKit metadata.
6. Progress events from ACP notifications are forwarded to `ProgressSink`.
7. Integration tests use ACP stub binary (like `tddy-acp-stub` pattern from `ClaudeAcpBackend` tests).
8. Old `CodexBackend` remains functional alongside new backend.

## Success Criteria

- All existing tests pass (no regressions).
- New integration tests cover: basic invoke, resume, OAuth URL capture, error handling.
- `codex-acp` backend works end-to-end with real `codex-acp` runtime.
- OAuth flow works through tddy-web dashboard when using `--agent codex-acp`.

## References

- [Migration plan](../../../../plans/codex-backend-migrate-to-codex-acp.md)
- [codex-acp repository](https://github.com/zed-industries/codex-acp)
- [Agent Client Protocol](https://agentclientprotocol.com)
