# 2026-07-11 — Subagent token usage

**Type:** Feature

`TokenUsage` (`total()`, field-wise `Add`) + optional `usage` on `ChatCompletionResponse` (from OpenAI/Ollama `prompt_tokens`/`completion_tokens`; absent → `None`, partial → zeros); `SubagentSession` gains `model()`/`cumulative_usage()`, `PromptOutcome` carries per-call `usage`, and both session impls accumulate per-turn usage into a running total. Feature [session-token-accounting.md](../../../../docs/ft/coder/session-token-accounting.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#289](https://github.com/uppin/tddy-coder/pull/289). (tddy-discovery)
