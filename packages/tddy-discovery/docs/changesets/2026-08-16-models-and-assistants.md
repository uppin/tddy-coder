# 2026-08-16 — models-and-assistants

**Type:** Feature

`SubagentTool` widens from 6 variants to the full 10-tool exec catalog (`Shell`, `Await`, `ReadLints`, `SemanticSearch` added, `Shell` joining the mutating set and Managed-only), with `catalog_name`/`from_catalog_name` so assistants and YAML subagents share one vocabulary. `SpecializedAgentDef` gains an `api_key` redacted in `Debug`, threaded into `OpenAiClient` at all three consumer sites — without it an assistant on a keyed provider started successfully and 401'd on every call. `OpenAiClient` gains an optional bearer credential and configurable connect/request timeouts; `engine_tool_definitions()` advertises the four newly covered tools. (tddy-discovery)
