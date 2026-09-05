# 2026-06-24 — FastContext Discovery agent + tddy-graph extraction

- New `--agent fastcontext` backend: drives `microsoft/FastContext-1.0-4B-RL` via an OpenAI-compatible `/v1/chat/completions` API for codebase exploration (READ/GLOB/GREP tool loop, `<final_answer>` citations)
- `ToolExecutor` runs tools locally (`std::fs`/`glob`/`regex`) or remotely via the existing `ExecuteTool` RPC; `is_error` responses surface as errors with no silent fallback
- Citations in `<final_answer>` blocks (`path:N-M` lines) are parsed into `DiscoveryData.relevant_code`; malformed lines excluded
- Configure with `--fastcontext-url <url>` (default `http://localhost:30000`) and `--fastcontext-max-turns <n>` (default 10); registered in `dev.daemon.yaml` `allowed_agents`
- `ChatMessage` named constructors (`user`/`system`/`assistant`/`tool_result`) eliminate struct-literal repetition across the multi-turn loop
- Prerequisite: `tddy-graph` extracted from `tddy-core` — standalone crate with no `tddy-*` dependencies; all existing consumer import paths preserved via re-export shim; `RunnerHooks` gains `on_enter_task`/`on_exit_task` lifecycle callbacks replacing backend-typed sink methods
