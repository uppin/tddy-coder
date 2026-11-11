# Session Token Accounting

**Product area:** coder / sandbox session

A `tddy-sandbox-app` session runs several distinct conversations against language models, and
accounts the tokens each spends. When the session ends it prints a per-conversation breakdown to
stderr, and the accounting is also exposed at the RPC/backend layer.

## Where an agent's usage is declared

Each coding agent spec declares its own usage source under `extensions.coding_agent.usage` — a list
of `sources`, each naming a **format** (`jsonl`) and how to read it:

- **`main.path`** — a path template for the conversation's own file(s); every match is merged into
  one row. Variables: `{agent_home}`, `{session_dir}`, `{conversation_id}`, `{worktree}`. A variable
  with no value for the session matches no files.
- **`children`** (optional) — a template over each conversation file's `{dir}` and `{stem}`, applied
  recursively (to depth 16, never revisiting a file). Each match is a child row whose `parentId` is
  its parent's id. `id` is a template; `name` may come from a sidecar JSON file's field, with a
  default. Siblings are ordered by id.
- **`record`** — `where` (field-equality predicates), `dedupe_by` (the last line per key wins; a line
  without the key is its own record), `aggregate: sum`, `model` (the last value seen), `tokens` (a
  field path per counter: `input`, `cache_read`, `cache_write`, `output`, `reasoning`) and an
  optional `cost`. `turns` is the number of kept records.

A spec with an unknown format, an unknown template variable, an empty `sources`, an empty field path
or a leftover `transcript_usage` key is **refused at load**, naming the file. A session whose agent
declares no `usage:` has no main-agent row — only its tddy subagent rows.

## Conversations accounted

- **Main `claude` / `claude-acp` agent** — `claude.yaml` and `claude-acp.yaml` bind
  `{agent_home}/.claude/projects/*/{conversation_id}.jsonl`, keep `type == assistant` lines, and
  **deduplicate by `message.id`** (Claude Code writes one line per content block, each repeating the
  message's usage). Counters: `message.usage.input_tokens` → input,
  `cache_read_input_tokens` → cache read, `cache_creation_input_tokens` → cache write,
  `output_tokens` → output; model from `message.model`.
- **Claude's nested Task-tool subagents** — `{dir}/{stem}/subagents/agent-*.jsonl`, named from the
  sibling `agent-<id>.meta.json` `agentType` (default `subagent`), found recursively: a subagent of a
  subagent is a row under its parent.
- **tddy `subagent_*` conversations** (`fastcontext` and other YAML-defined specialized agents,
  including local models via **Ollama**) — accumulated from the model's own `usage` object across
  every prompt turn.

## Behavior

- **List all conversations (RPC).** The in-jail `tddy-tools --mcp` server exposes a `subagent_list`
  tool returning every open subagent conversation with `{ agent, id, model, inputTokens,
  outputTokens, totalTokens, turns, cacheReadTokens, cacheWriteTokens, reasoningTokens, costUsd?,
  parentId? }` (every field after `turns` defaulted on read, so an older file still reads), and writes the same list to a host-visible accounting file
  (`TDDY_TOOLS_ACCOUNTING_FILE`, pointed by the runner at `<session>/egress/accounting.json`) on
  each prompt/cancel. `subagent_prompt` results also carry the turn's `usage`.
- **Subagent usage** is read from the model's `usage` (`prompt_tokens` → input, `completion_tokens`
  → output) on `/v1/chat/completions`, which OpenAI-compatible endpoints and Ollama both report. A
  response that omits `usage` counts as zero — it never fails a turn.
- **End-of-session summary.** After the terminal bridge returns, `tddy-sandbox-app` prints one row
  per conversation — the main agent, its Task subagents, then the tddy subagents — followed by a
  session TOTAL row, to stderr — `in`, `cache_read`, `cache_write`, `out`, `reasoning`, `total` and
  `turns` per row. An agent with no `usage:` binding skips the main-agent row. A missing accounting
  file contributes no tddy-subagent rows; a missing transcript yields a zero main-agent row (with
  the model from CLI args), never an error.
- **Disjoint counters.** `input` is fresh input only and `output` excludes reasoning, so
  `total = input + cache_read + cache_write + output + reasoning`. A source's `cost` is carried
  when it reports one; nothing estimates it.

## Where it lives

- `tddy-discovery` — `TokenUsage` and `usage` parsing on `ChatCompletionResponse`; per-turn and
  cumulative usage on `SubagentSession` (`model()` / `cumulative_usage()`, `PromptOutcome.usage`).
- `tddy-usage` — the binding model, its evaluation (`account`, `gather_session_usage`) and
  `token_accounting` (`TokenUsage`, `ConversationRecord`, `format_token_summary`); it names no agent.
  `tddy-agent-backend::token_accounting` re-exports it.
- `tddy-coding-agent-specs` — parses and validates `coding_agent.usage`; the Claude bindings in
  `specs/claude.yaml` and `specs/claude-acp.yaml`.
- `tddy-tools` — `subagent_list` tool, `usage` in `subagent_prompt` results, accounting-file writer.
- `tddy-sandbox-runner` — sets `TDDY_TOOLS_ACCOUNTING_FILE` on the in-jail MCP spawn.
- `tddy-sandbox-app` — `print_token_summary` gathers the agent's binding plus the accounting file and
  prints the breakdown.
- `tddy-agent-launch` / `tddy-spawn` — every launch, resume and relaunch records the session's usage
  context (`<session_dir>/usage-context.json`: spec id, agent home as a host path, conversation id,
  worktree, model; `tddy_usage::write_usage_context`).
- `tddy-session-activity` — `ActivityService.StreamSessionUsage`: one poller per watched session
  evaluates the spec's binding against that context (`tddy_usage::gather_session_usage_for`) and
  streams each changed snapshot to the web's Usage tab
  ([Session Usage Inspector](../web/session-usage-inspector.md)).

## Scope

- **In scope:** RPC/backend layer + the `tddy-sandbox-app` CLI stderr summary.
- **In scope (web):** the daemon streams every session's usage to the Inspector's Usage tab — see
  [Session Usage Inspector](../web/session-usage-inspector.md).
- **Out of scope:** TUI visualization; USD cost estimation; bindings for agents other than Claude and
  Claude-ACP (later `#usage-binding` nodes).

> ⚠️ The live end-to-end stderr summary against a real Ollama-backed session has not yet been
> exercised; the behavior is covered by unit/acceptance tests (including against a real Claude
> transcript layout on disk).
