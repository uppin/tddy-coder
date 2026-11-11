# 2026-11-11 — Session types come from YAML declarations

**Type:** Fix

- `session_list_enrichment` and `resolve_session_label` take the host's `CodingAgentRegistry`: a CLI session is one whose type a coding agent declares, not one ending `-cli`. Worktree removal checks `workspace` through `is_no_agent_session_type`.

Cross-package: [declared session types](../../../../docs/dev/changesets/2026-11-11-declared-session-types.md). Product: [Session types are declared](../../../../docs/ft/daemon/session-agent-roster.md#session-types-are-declared).
