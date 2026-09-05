# 2026-07-23 — session-activity-streaming-mode: `proto/connection.proto` adds `enum StreamMode { SNAPSHOT_THEN_LIVE, LIVE_ONLY }` + `StreamSessionActivityRequest.mode` (field 4), and changes `AgentActivityRecord.input`/`result` (fields 3/5) from `string` to `google.protobuf.Value`. New `lib.rs` helpers `json_to_proto_value` + `agent_activity_to_proto` (record → wire) with 7 converter unit tests. Regenerated Rust + `tddy-web/src/gen/connection_pb.ts`. Feature [agent-activity-pane.md](../../../../docs/ft/web/agent-activity-pane.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)

**Type:** Feature


