# 2026-03-07 — Backend Abstraction Refactor (OCP)

**Type:** Feature

InvokeRequest slim (Goal enum, no permission_mode/allowed_tools/mcp_config_path). InvokeResponse session_id Option. ClaudeCodeBackend maps goal→ClaudeInvokeConfig internally. CursorBackend added. Stream split: stream/claude.rs (Claude NDJSON), stream/cursor.rs (Cursor NDJSON). AnyBackend enum dispatch. CLI --agent and --prompt flags. append_session_and_update_state takes agent param. parse_clarification_questions_from_text fallback for Cursor. (tddy-core)
