# 2026-07-02 — Removed the `discovery_subagent` single-name alias entirely

**Type:** Fix

no backwards compatibility retained. `discovery_subagent` (proto `StartSessionRequest` field 19 — now `reserved`, `SessionMetadata`, `tddy-daemon` request handling, `tddy-sandbox-app`'s `--discovery-subagent` CLI flag) is deleted outright; `specialized_agents`/`--specialized-agent` (even a single-element array) is the only way to wire a subagent into a session, for both new-session start and resume. Follows immediately on the prior entry below, which removed the per-field config overrides but left the name-only alias in place. Feature [managed-codebase-subagents.md](../../ft/coder/managed-codebase-subagents.md) AC 24. (tddy-service, tddy-core, tddy-daemon, tddy-sandbox-app, tddy-testing-commons)
