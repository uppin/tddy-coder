# 2026-07-24 — activities-as-acp: `ConnectionService` gains `stream_acp_replay` (+ `MpscAcpReplayStream`, `acp_replay_frame`, `relay_acp_replay`)

**Type:** Feature

snapshot via `tddy_service::acp_replay::read_acp_transcript` then live `AgentActivityHub` tail (mapped to `AcpReplayFrame`), honouring `StreamMode` and mirroring `stream_session_activity` (Local-only; `PeerRoute::Forward` → `unimplemented`). Tonic adapter gains the `StreamAcpReplayStream` type + shim. Also fixed a pre-existing broken test target: `session_catalog_populate.rs` now calls `tddy_bsp::register_catalog_provider()` (was the non-existent `tddy_coder::catalog_provider::register()`). Feature [agent-activity-pane.md](../../../../docs/ft/web/agent-activity-pane.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon)
