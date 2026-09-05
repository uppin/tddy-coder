# 2026-07-02 — `ListSubagents` RPC + specialized-agent env wiring for sandboxed claude-cli sessions

**Type:** Feature

new `ListSubagents` RPC returns resolved `SpecializedAgentDef`s (builtin + `<tddyhome>/agents`) as `SubagentInfo` rows; `StartSessionRequest.managed_codebase`/`.specialized_agents` accepted for `session_type == "claude-cli"` sandboxed sessions — new `ConnectionServiceImpl::specialized_subagent_env` resolves named agents (an unresolvable name is `INVALID_ARGUMENT`) into `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON` jail env pairs; the sandboxed path already never mounts the repo, so `managed_codebase` is accepted for request-shape clarity rather than toggling mounts. Feature [specialized-subagents.md](../../../../docs/ft/coder/specialized-subagents.md); technical [connection-service.md](../connection-service.md#sandboxed-claude-code-cli-sessions). (tddy-daemon, tddy-discovery, tddy-service)
