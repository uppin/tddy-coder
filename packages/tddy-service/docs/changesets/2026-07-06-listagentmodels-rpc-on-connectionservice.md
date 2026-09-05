# 2026-07-06 — `ListAgentModels` RPC on `ConnectionService`

**Type:** Feature

`proto/connection.proto` adds a session-token-validated unary `ListAgentModels(ListAgentModelsRequest{session_token, agent, daemon_instance_id})` returning `ListAgentModelsResponse{repeated ModelInfo models, string default_model}` with new `ModelInfo{id, label}`, letting the web enumerate a backend's models on demand when creating a session. `StartSessionRequest.model` (field 8) is reused unchanged (now populated for tool sessions, not just claude-cli); `AgentInfo`/`ListAgents` untouched. Regenerated Rust + `tddy-web/src/gen/connection_pb.ts`. Feature [tool-session-model-selection.md](../../../../docs/ft/web/tool-session-model-selection.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)
