# 2026-11-11 — Session types come from YAML declarations

**Type:** Fix

- `catalog.proto`: `ListAgentsResponse.session_types` (`SessionTypeInfo { session_type, label, launch }`). `session.proto`: `StartSessionRequest.session_type` is required and must be declared or `workspace`.

Cross-package: [declared session types](../../../../docs/dev/changesets/2026-11-11-declared-session-types.md). Product: [Session types are declared](../../../../docs/ft/daemon/session-agent-roster.md#session-types-are-declared).
