# 2026-07-19 — `spawn_conversation` MCP tool + `spawn-conversation` CLI

**Type:** Feature

`server.rs` adds `SpawnConversationInput { prompt, branch: Option, base_ref: Option }` and a `spawn_conversation` `#[tool]` (not pr-stack-gated) that guards on `permission_relay_socket_path()`, builds the wire request via a pure, unit-testable `spawn_conversation_request_json`, and relays over `TDDY_SOCKET` via `toolcall_client::dispatch_toolcall`. Reverse-RPC path adds the kebab `spawn-conversation` CLI subcommand + `spawn-conversation → SpawnConversation` wire mapping (grill-me prompt uses the kebab name). Tests: errors without `TDDY_SOCKET` + relayed request shape (2). Feature [spawn-conversation.md](../../../../docs/ft/coder/spawn-conversation.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-tools)
