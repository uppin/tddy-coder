# 2026-07-02 — ACP-shaped discovery-subagent MCP tools

**Type:** Feature

`PermissionServer` merges `subagent_new_session`/`subagent_prompt`/`subagent_cancel` (mirroring ACP's `session/new`/`session/prompt`/`session/cancel`) into its `tools/list` when `TDDY_SUBAGENT` is set, backed by a process-wide session table (`tokio::sync::Mutex<HashMap<sessionId, Box<dyn SubagentSession>>>`) so a conversation survives across separate `tools/call` invocations; the caller-supplied `sessionId` is honored verbatim. New env vars `TDDY_SUBAGENT`, `TDDY_SUBAGENT_FASTCONTEXT_{URL,MODEL,MAX_TURNS}`, `TDDY_SUBAGENT_CODEBASE_ACCESS`. New `tddy-discovery` + `uuid` dependencies. Feature [managed-codebase-subagents.md](../../../../docs/ft/coder/managed-codebase-subagents.md). (tddy-tools, tddy-discovery, tddy-sandbox-recipes, tddy-sandbox-app, tddy-sandbox-runner)
