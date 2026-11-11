# 2026-11-10 — tddy-coding-subagent: usage from the binding

`CodingAgentLaunch::session_dir`; `UsageReading` and `CodingAgentSession::with_usage_reading` — `cumulative_usage` reads the conversation's row under its agent's binding (zero without one); `context_tokens` stays 0. `#usage-binding 3/5`, PR [#892](https://github.com/uppin/tddy-coder/pull/892).
