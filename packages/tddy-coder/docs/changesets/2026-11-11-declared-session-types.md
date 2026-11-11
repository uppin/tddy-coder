# 2026-11-11 — Session types come from YAML declarations

**Type:** Fix

- Every `.session.yaml` tddy-coder starts records the session type its host declares for the `tddy_coder` launch (`declared_tool_session_type`, from `CodingAgentRegistry::sole_session_type_launching` over `<tddyhome>/tools` — `tool`); a host declaring none or several is refused.

Cross-package: [declared session types](../../../../docs/dev/changesets/2026-11-11-declared-session-types.md). Product: [Session types are declared](../../../../docs/ft/daemon/session-agent-roster.md#session-types-are-declared).
