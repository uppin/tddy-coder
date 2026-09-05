# 2026-07-11 — Token accounting module + Claude transcript readers

**Type:** Feature

agent-neutral `token_accounting` (`TokenUsage`, `ConversationRecord`, `format_token_summary`) plus Claude-backend readers `read_claude_transcript_usage` (main thread `<session_id>.jsonl`) and `read_claude_subagent_usages` (one record per nested Task subagent under `<session_id>/subagents/agent-*.jsonl`, agent name from `.meta.json` `agentType`); `cache_*` counters excluded. Feature [session-token-accounting.md](../../../../docs/ft/coder/session-token-accounting.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#289](https://github.com/uppin/tddy-coder/pull/289). (tddy-core)
