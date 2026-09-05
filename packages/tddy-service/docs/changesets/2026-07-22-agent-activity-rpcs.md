# 2026-07-22 — **Agent-activity RPCs

**Type:** Feature

`StreamSessionActivity` / `ReportAgentActivity` on `ConnectionService`** — `proto/connection.proto` adds `AgentActivityRecord`, `StreamSessionActivityRequest`, a **server-streaming** `StreamSessionActivity` (replays the coalesced snapshot then tails live agent tool calls), and a unary `ReportAgentActivity` (claude-cli hook → daemon). Regenerated Rust + `tddy-web/src/gen/connection_pb.ts`. Feature [agent-activity-pane.md](../../../../docs/ft/web/agent-activity-pane.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)
