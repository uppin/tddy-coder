# 2026-11-10 — tddy-session-agents: coding subagents are registered for usage

`open_coding_agent_session` registers each coding-subagent conversation under the session's main conversation (`usage-conversations.jsonl`) and attaches its `UsageReading`; an ACP open carries the tap from `acp.usage_events` (not yet for a `shared` placement). `#usage-binding 3/5`, PR [#892](https://github.com/uppin/tddy-coder/pull/892).
