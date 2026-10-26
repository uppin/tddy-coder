# 2026-10-25 — spawn_conversation starts any session through the universal start

**Type:** Feature

`#start-parity` 9/9 ([#758](https://github.com/uppin/tddy-coder/pull/758)). `spawn_conversation` was a
second, narrower start path (grill-me only, `claude-cli` only, local only, no progress, no cancel). It
now takes `StartSession`'s arguments (`StartSessionArgs`), runs the start the project-home
`StartSession` tool runs, records it in the orchestrator's start ledger and answers with
`{start_id, task_id, daemon_instance_id}`; `spawn_conversation_status` / `spawn_conversation_cancel`
(MCP) and `tddy-tools spawn-conversation-status` / `-cancel` (CLI) follow and stop it. A session that
can spawn conversations (`SessionToolRole::ConversationSpawner`, the `grill-me` recipe) is offered
and served the four start tools for its own project. Decisions: the start authenticates as the
orchestrator's own session token (a session with none starts nothing — a refusal, not a fallback);
nothing is inherited (a `claude-cli` conversation names its model, the link is
`orchestrator_session_id`); the legacy `SpawnConversation` verb stays for older `tddy-tools`. Across:

- `tddy-agent-launch` — `conversation_start.rs` (`LaunchSessions::begin_recorded_start` /
  `cancel_recorded_start`, `StartedSessionListings`, moved out of `tddy-daemon-rpc`'s project-home
  port and shared); `GrillMeConversationSpawnHandler.service` and its three universal-start methods;
  `session_tool_role`; `SessionAccountAccess::session_token`.
- `tddy-tool-engine` — `StartLedger::recovered_in_session_dir` (the once-per-process recovery, shared);
  `SessionToolRole`, `is_conversation_spawner_tool`, `session_offered_catalog_for`.
- `tddy-toolcall` — `ConversationSpawnHandler::{start_conversation, conversation_start_status,
  cancel_conversation_start}` (default bodies refuse), the `StartConversation` /
  `ConversationStartStatus` / `ConversationStartCancel` verbs, `ToolCallResponse::ConversationStartJson`;
  the conversation wire types now live in `toolcall/conversation_wire.rs`.
- `tddy-host-service` / `tddy-coder` — the three methods on `HostSessionService` and the stdio relay.
- `tddy-tools` / `tddy-tools-cli` — the MCP tools and CLI subcommands; the CLI family is
  `cli/spawn_conversation.rs`. `tddy-workflow-recipes` — the grill-me hand-off prompt.
- `tddy-daemon-rpc` — the project-home port delegates its start and cancel; `project_home_caller` and
  `ListExecTools` admit a conversation spawner for the four start tools.

Restructuring (`/pr-wrap` 3.5, engine `move_item`): `tddy-toolcall/src/toolcall/mod.rs` 469 → 508 →
476 (the three conversation wire types to `conversation_wire.rs`), `tddy-tools/src/cli.rs` 459 → 509 →
424 (the spawn-conversation CLI family to `cli/spawn_conversation.rs`).

Code issues (recorded, split deferred by developer instruction): `tddy-daemon-rpc/src/exec_tool/project_home.rs`
788 → 641 (better: the start moved out), `tddy-tool-engine/src/project_home_tools.rs` 660 → 699,
`tddy-toolcall/src/toolcall/listener.rs` 766 → 879, `tddy-tools/src/server.rs` 3,037 → 3,085.
TODO added: "A conversation spawner whose call is forwarded from another host is refused the start
tools" (`2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools`).
