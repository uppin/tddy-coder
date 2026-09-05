# 2026-07-19 — `spawn_conversation` toolcall verb + `ConversationSpawnHandler`

**Type:** Feature

`toolcall/mod.rs` adds `SpawnConversationRequestWire { prompt, branch: Option, base_ref: Option }` (parallel to `SpawnChildRequestWire`, reusing `ToolCallResponse::SpawnChildOk`); `toolcall/listener.rs` adds the `ConversationSpawnHandler` trait + `with_conversation_spawn_handler` builder + `conversation_spawn_handler` field, `"SpawnConversation"` in the dispatch allowlist, and `handle_spawn_conversation` (rejects with a message when unbound). Reverse-RPC support: `start_toolcall_listener_with_conversation_handler` (3-arg form delegates `None`) + shared `HOST_SESSION_SERVICE`/`SPAWN_CONVERSATION_METHOD` consts. 4 dispatch/wire units (258/0 suite). Feature [spawn-conversation.md](../../../../docs/ft/coder/spawn-conversation.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-core)
