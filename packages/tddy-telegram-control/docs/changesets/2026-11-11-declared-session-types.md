# 2026-11-11 — Session types come from YAML declarations

**Type:** Fix

- The agent keyboard and the `/start-claude` / `/start-cursor` shortcuts take each agent's session type from its declaration (`session_type_of`); a pending session records that value. Listing enrichment passes the host's coding agents.

Cross-package: [declared session types](../../../../docs/dev/changesets/2026-11-11-declared-session-types.md). Product: [Session types are declared](../../../../docs/ft/daemon/session-agent-roster.md#session-types-are-declared).
