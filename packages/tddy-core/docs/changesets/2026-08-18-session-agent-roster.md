# 2026-08-18 — **session agent roster

**Type:** Feature

`AgentId`, `SessionAgentRecord`, and the `SessionMetadata` field swap** — new `session_agent` module: `AgentId` (parse/format `name@daemon_instance_id`, refusing a name containing `@` so a formatted id always parses back), and `SessionAgentRecord` persisted in `.session.yaml`. `SessionMetadata` swaps `specialized_agents: Vec<String>` for `agents: Vec<SessionAgentRecord>` + `agents_rev: u64`; a `legacy_specialized_agents` tombstone keeps `deny_unknown_fields` parsing pre-roster `.session.yaml` (read and discarded, never written back, never consulted). Feature [session-agent-roster.md](../../../../docs/ft/daemon/session-agent-roster.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-core)
