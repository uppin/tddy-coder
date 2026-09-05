# 2026-07-02 — Multi-agent `TDDY_SUBAGENTS_JSON` support for the subagent MCP tools

**Type:** Feature

`subagents_from_env` parses `TDDY_SUBAGENTS_JSON` (a JSON array of `SpecializedAgentDef`) and, when present, builds the `SubagentRegistry` via `from_defs` instead of the single hardcoded `"fastcontext"` factory, so `subagent_new_session { agent: "<name>" }` can select among any number of configured agents; `TDDY_SUBAGENT=fastcontext` alone (no `TDDY_SUBAGENTS_JSON`) still resolves via the legacy `SubagentRegistry::new()` path unchanged. Feature [specialized-subagents.md](../../../../docs/ft/coder/specialized-subagents.md). (tddy-tools, tddy-discovery)
