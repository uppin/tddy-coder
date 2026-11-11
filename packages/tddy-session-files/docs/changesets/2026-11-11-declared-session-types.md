# 2026-11-11 — Session types come from YAML declarations

**Type:** Fix

- `SessionContextAgent::Own` carries the recorded session type; the caller resolves it to the declaring agent (`for_session_type`) instead of stripping `-cli`.

Cross-package: [declared session types](../../../../docs/dev/changesets/2026-11-11-declared-session-types.md). Product: [Session types are declared](../../../../docs/ft/daemon/session-agent-roster.md#session-types-are-declared).
