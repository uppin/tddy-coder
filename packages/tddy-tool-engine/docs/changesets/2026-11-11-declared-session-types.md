# 2026-11-11 — Session types come from YAML declarations

**Type:** Fix

- `StartChoices.session_types` (`SessionTypeChoice`) feed the StartSession schema's `session_type` enum plus `workspace`; without choices the field is a free string pointing at `DescribeSessionStartOptions`. The `tool_path` description states it is required for a tddy-coder session type and must be in `allowed_tools`.
- `CandidateSession.session_types`; `provider_harness` and `models_agent` tell a coding agent's session type from the injected `AgentChoice`s and the no-agent type through `is_no_agent_session_type` — no `"tool"` / `"workspace"` literals.

Cross-package: [declared session types](../../../../docs/dev/changesets/2026-11-11-declared-session-types.md). Product: [Session types are declared](../../../../docs/ft/daemon/session-agent-roster.md#session-types-are-declared).
