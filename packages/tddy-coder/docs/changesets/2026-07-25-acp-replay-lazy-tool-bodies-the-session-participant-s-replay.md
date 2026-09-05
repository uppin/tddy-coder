# 2026-07-25 — acp-replay-lazy-tool-bodies: the session participant's `replay_frame_bytes` strips tool bodies via `tddy_service::acp_replay::strip_tool_body` (snapshot + live presenter tail), and a new `GetAcpToolCallDetail` `handle_rpc` arm returns one call's bodies via `tool_call_detail(&self.svc.agent_activity_dir, …)` (`NOT_FOUND` for unknown id). Tests: `stream_acp_replay_snapshot_frames_omit_tool_bodies` + `get_acp_tool_call_detail_*` 3. Feature [acp-replay-lazy-tool-bodies.md](../../../../docs/ft/coder/acp-replay-lazy-tool-bodies.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-coder)

**Type:** Feature


