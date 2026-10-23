# 2026-10-23 — managed_codebase recorded at start

**Type:** Feature

`#project-home` 3/7 (#698). Every start path writes `managed_codebase` into the initial `.session.yaml`: co-located claude-cli (`spawn_claude_cli_session_inner` gains the parameter, threaded from `cli_branch_starts.rs`), sandboxed claude-cli (`write_jail_session_metadata`), co-located and sandboxed cursor-cli; child and conversation spawns record `false`. `session_coordinate_handlers.rs` copies it into the proto `SessionEntry`. `start_session_core` gained no branch. Code issues touched, unchanged in shape: `start_sandboxed_claude_cli_session` 348 → 349 lines, `start_sandboxed_cursor_cli_session` 430 → 431 (one pass-through line each).
