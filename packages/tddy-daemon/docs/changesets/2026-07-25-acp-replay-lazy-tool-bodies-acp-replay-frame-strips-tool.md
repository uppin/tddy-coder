# 2026-07-25 — acp-replay-lazy-tool-bodies: `acp_replay_frame` strips tool bodies via `tddy_service::acp_replay::strip_tool_body` (covers the `StreamAcpReplay` snapshot loop + live `relay_acp_replay` tail; `SNAPSHOT_THEN_LIVE`/`LIVE_ONLY` body-less, `COUNT_THEN_LIVE` unchanged), and new unary `get_acp_tool_call_detail` returns one call's `raw_input`/`raw_output` from `tool_call_detail` (auth + `ExecuteTool`-style peer-forward, `NOT_FOUND` for unknown id). Tests: `stream_acp_replay_*_omit_tool_bodies` 2 + `get_acp_tool_call_detail_*` 2. Docs [connection-service.md](../connection-service.md#agent-activity-log--stream). Cross-package [changeset](../../../../docs/dev/changesets/). (tddy-daemon)

**Type:** Feature


