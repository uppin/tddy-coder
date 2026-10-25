# 2026-10-24 — A coding agent's tddy-tools server
**Type:** Feature

`coding_agent_wiring::CodingAgentToolHost::tool_server` builds the `tddy-tools --mcp` environment a managed coding agent is handed: the colocated jail's `TDDY_REMOTE_*` variables and a token minted for the agent; a daemon that signs no tokens refuses (#721, `#coagents` 2/8).
