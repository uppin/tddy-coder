# 2026-10-24 — Agent terminal tabs: `start_agent_terminal`
**Type:** Feature

`CliSessionManager::start_agent_terminal(session_id, worktree, kind, argv)` spawns a prebuilt harness argv through `spawn_tool` under a fresh UUIDv7 terminal id, labelled with the harness's session type as its kind, with no session environment (`cli_session_manager/terminals.rs`, #718).
