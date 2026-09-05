# 2026-07-02 — `connection.proto`: `ListSubagents` RPC + `StartSessionRequest` specialized-agent fields

**Type:** Feature

new `ListSubagents(ListSubagentsRequest) returns (ListSubagentsResponse)` + `SubagentInfo { name, label, model }`; `StartSessionRequest` gains field 17 `managed_codebase` (bool) and field 18 `specialized_agents` (repeated string). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). Feature [specialized-subagents.md](../../../../docs/ft/coder/specialized-subagents.md). (tddy-service, tddy-daemon, tddy-web)
