# 2026-11-11 — Session types come from YAML declarations

**Type:** Fix

- `ListAgents` serves `ListAgentsResponse.session_types` (`session_type_infos`); `AgentInfo.session_type` is the spec's declared one.
- `DescribeSessionStartOptions` answers `session_types` and builds its agent choices from the declared `agent_cli` session types; `models_agent` / `provider_harness` take the injected agent choices.

Cross-package: [declared session types](../../../../docs/dev/changesets/2026-11-11-declared-session-types.md). Product: [Session types are declared](../../../../docs/ft/daemon/session-agent-roster.md#session-types-are-declared).
