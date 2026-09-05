# 2026-07-02 — Stateful discovery subagent sessions (ACP-shaped)

**Type:** Feature

new `subagent.rs`: `SubagentSession` trait, `CodebaseAccess { Local | Managed(injected dispatch fn) }` (lets a subagent read the host filesystem directly or through a caller-injected proxy dispatch closure without depending on `tddy-tools`/`tddy-rpc`/`tddy-stdio`), stateful `FastContextSession` (owns message history across `prompt()` calls, per-call turn budget), pluggable `SubagentRegistry` (name → factory, `"fastcontext"` built in). Extracted `openai::discovery_tool_definitions()` as the single shared READ/GLOB/GREP tool-schema source for both `FastContextBackend::invoke` (one-shot) and `FastContextSession::prompt` (stateful), removing a ~50-line byte-identical duplication. Feature [managed-codebase-subagents.md](../../../../docs/ft/coder/managed-codebase-subagents.md). (tddy-discovery, tddy-tools)
