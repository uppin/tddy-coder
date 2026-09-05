# 2026-07-02 — Removed legacy per-agent config overrides from the subagent-wiring surface

**Type:** Fix

`fastcontext_url`/`fastcontext_model`/`fastcontext_max_turns`/`subagent_replaces` (CLI flags on `tddy-sandbox-app`, `StartSessionRequest` proto fields 20-23 in `tddy-service` — now `reserved` — daemon threading, and `SessionMetadata` fields) are deleted outright; `discovery_subagent` is now a name-only alias folded into the same YAML-resolved `specialized_agents` pipeline (`tddy-daemon::connection_service`) instead of its own parallel hardcoded-env path. All specialized-agent configuration (model, base_url, max_turns, replaces) comes exclusively from the resolved agent's YAML def (`<tddyhome>/agents/*.yaml`) or the builtin `fastcontext` def. Feature [managed-codebase-subagents.md](../../ft/coder/managed-codebase-subagents.md) AC 24. (tddy-service, tddy-core, tddy-daemon, tddy-sandbox-app, tddy-testing-commons)
