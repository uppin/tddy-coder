# 2026-07-25 — acp-replay-lazy-tool-bodies: `proto/connection.proto` adds unary `GetAcpToolCallDetail(session_token, session_id, daemon_instance_id, tool_call_id) → {optional raw_input, optional raw_output}`; `src/acp_replay.rs` adds `strip_tool_body` (clears a tool frame's `raw_input`/`raw_output`, keeps title/kind/status/id) and `tool_call_detail(session_dir, tool_call_id) → Option<ToolCallDetail>` (resolved from `read_session_transcript`). Tests: `acp_replay` strip/detail 5. Feature [acp-replay-lazy-tool-bodies.md](../../../../docs/ft/coder/acp-replay-lazy-tool-bodies.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)

**Type:** Feature


