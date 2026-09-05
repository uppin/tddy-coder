# 2026-07-11 — Session token accounting

- A sandbox session now accounts tokens **per conversation** and prints a per-agent breakdown to stderr when it ends: the main `claude` agent, each of Claude's nested Task-tool subagents (Explore/general-purpose/…), and every tddy `subagent_*` conversation — including local models via **Ollama**.
- Each row shows agent name, conversation id, model, cumulative input/output/total tokens, and turn count, followed by a session TOTAL. Tokens only — no cost estimation.
- New RPC surface: a `subagent_list` MCP tool enumerates all open subagent conversations with their token totals; the in-jail MCP server also writes the same list to `<session>/egress/accounting.json` (`TDDY_TOOLS_ACCOUNTING_FILE`).
- Subagent usage comes from the model's own `usage` (OpenAI/Ollama `/v1/chat/completions`); the main agent and its Task subagents are summed from Claude Code's transcript JSONL (`cache_*` counters excluded).
- RPC/backend layer only (no web/TUI dashboard). ⚠️ The live end-to-end stderr summary against a real Ollama-backed session has not yet been exercised.
- Feature: [session-token-accounting.md](../session-token-accounting.md)
