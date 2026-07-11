# Session Token Accounting

**Product area:** coder / sandbox session

A `tddy-sandbox-app` session runs several distinct conversations against language models, and
accounts the tokens each spends. When the session ends it prints a per-conversation breakdown to
stderr, and the accounting is also exposed at the RPC/backend layer.

## Conversations accounted

- **Main `claude` agent** — summed from Claude Code's own transcript JSONL. The runner spawns
  `claude --session-id <session_id>`, so the transcript path
  (`<claude_home>/.claude/projects/<encoded-cwd>/<session_id>.jsonl`) is deterministic. Input and
  output tokens are folded from each assistant message's `message.usage`; Claude's separate
  `cache_*` counters are not folded into input.
- **Claude's nested Task-tool subagents** (Explore / general-purpose / etc.) — each recorded by
  Claude Code under `<session_id>/subagents/agent-<id>.jsonl` with a sibling `agent-<id>.meta.json`.
  Each is accounted as its own conversation: agent name from the meta's `agentType`, id from the
  `agent-<id>` file stem, tokens/model from its own transcript.
- **tddy `subagent_*` conversations** (`fastcontext` and other YAML-defined specialized agents,
  including local models via **Ollama**) — accumulated from the model's own `usage` object across
  every prompt turn.

## Behavior

- **List all conversations (RPC).** The in-jail `tddy-tools --mcp` server exposes a `subagent_list`
  tool returning every open subagent conversation with `{ agent, id, model, inputTokens,
  outputTokens, totalTokens, turns }`, and writes the same list to a host-visible accounting file
  (`TDDY_TOOLS_ACCOUNTING_FILE`, pointed by the runner at `<session>/egress/accounting.json`) on
  each prompt/cancel. `subagent_prompt` results also carry the turn's `usage`.
- **Subagent usage** is read from the model's `usage` (`prompt_tokens` → input, `completion_tokens`
  → output) on `/v1/chat/completions`, which OpenAI-compatible endpoints and Ollama both report. A
  response that omits `usage` counts as zero — it never fails a turn.
- **End-of-session summary.** After the terminal bridge returns, `tddy-sandbox-app` prints one row
  per conversation — the main agent, its Task subagents, then the tddy subagents — followed by a
  session TOTAL row, to stderr. A Cursor session skips the main-agent row. A missing accounting
  file simply contributes no tddy-subagent rows; a missing Claude transcript yields a zero
  main-agent row (with the model from CLI args), never an error.
- **Tokens only** — input / output / total and turn count. No monetary cost estimation.

## Where it lives

- `tddy-discovery` — `TokenUsage` and `usage` parsing on `ChatCompletionResponse`; per-turn and
  cumulative usage on `SubagentSession` (`model()` / `cumulative_usage()`, `PromptOutcome.usage`).
- `tddy-core::token_accounting` — agent-neutral `TokenUsage`, `ConversationRecord`,
  `format_token_summary`.
- `tddy-core::backend` (Claude backend) — `read_claude_transcript_usage`,
  `read_claude_subagent_usages` (Claude-Code-specific transcript layout lives with the backend).
- `tddy-tools` — `subagent_list` tool, `usage` in `subagent_prompt` results, accounting-file writer.
- `tddy-sandbox-runner` — sets `TDDY_TOOLS_ACCOUNTING_FILE` on the in-jail MCP spawn.
- `tddy-sandbox-app` — `print_token_summary` merges the sources and prints the breakdown.

## Scope

- **In scope:** RPC/backend layer + the `tddy-sandbox-app` CLI stderr summary.
- **Out of scope:** web dashboard / TUI visualization; USD cost estimation.

> ⚠️ The live end-to-end stderr summary against a real Ollama-backed session has not yet been
> exercised; the behavior is covered by unit/acceptance tests (including against a real Claude
> transcript layout on disk).
