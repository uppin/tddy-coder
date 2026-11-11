# Session Token Accounting

**Product area:** coder / sandbox session

A `tddy-sandbox-app` session runs several distinct conversations against language models, and
accounts the tokens each spends. When the session ends it prints a per-conversation breakdown to
stderr, and the accounting is also exposed at the RPC/backend layer.

## Where an agent's usage is declared

Each coding agent spec declares its own usage source under `extensions.coding_agent.usage` — a list
of `sources`, each naming a **format** (`jsonl` or `sqlite`) and how to read it. The fields below are
the `jsonl` format's; [the `sqlite` format](#the-sqlite-format) follows.

- **`main.path`** — a path template for the conversation's own file(s); every match is merged into
  one row. Variables: `{agent_home}`, `{session_dir}`, `{conversation_id}`, `{worktree}`. A variable
  with no value for the session matches no files.
- **`children`** (optional) — a template over each conversation file's `{dir}` and `{stem}`, applied
  recursively (to depth 16, never revisiting a file). Each match is a child row whose `parentId` is
  its parent's id. `id` is a template; `name` may come from a sidecar JSON file's field, with a
  default. Siblings are ordered by id.
- **`identity`** (optional) — `{ id, parent }` field paths read from the first of a file's leading
  lines (the first 50) that holds them, for an agent whose files name themselves rather than being named by their path.
  With an identity, `main.path` is a glob over every conversation file, the conversation id comes
  from the file, and the session's `conversation_id` is not needed.
- **`main.match`** (optional, needs `identity`) — `{ field, equals }`: the main conversation is the
  **newest** file (by modification time) whose header `field` equals the rendered `equals` template
  and which declares no parent. Without it, every file `main.path` matches is one row.
- **`children_by: parent_field`** (needs `identity`, replaces `children`) — every file whose
  `identity.parent` is a conversation's id is that conversation's child, recursively (to depth 16,
  never revisiting a conversation). The child's id is its own; siblings are ordered by id.
- **`record`** — `where` (field-equality predicates), `dedupe_by` (the last line per key wins; a line
  without the key is its own record), `aggregate` (`sum`, the default, or `last`), `model`, `tokens`
  and an optional `cost`. `turns` is the number of kept records.
  - **`aggregate: last`** — each counter (and the cost) is its value on the last kept record that
    carries it, for a source whose figures are already cumulative.
  - **Counter expressions** — a counter is a field path, or paths joined by `+` and `-`
    (`a.input - a.cached`): a missing operand counts as 0 and a negative result saturates at 0 with a
    logged warning. This is how a source whose counters overlap (cached input inside input, reasoning
    inside output) is mapped onto the record's disjoint counters. A field key containing `+` or `-` cannot be
    written in an expression.
  - **`model`** is read from **every** line of the file, including those `where` drops, and is the
    last value seen — so a line that only announces the model (not a usage line) still supplies it.

### The `sqlite` format

A store with one row per conversation. The binding names the table and the columns; no SQL is written
in it, and every table and column name must be a plain identifier (letters, digits and `_`, not
starting with a digit) or the spec is refused at load.

- **`path`** — a template over the main variables, the database file.
- **`table`**, **`id`**, optional **`parent`** — the conversation table, the id column and the column
  holding a row's parent id.
- **`root: { column, equals, newest_by }`** — the main conversation is the row whose `column` equals
  the rendered `equals` template, with no parent, newest by `newest_by`.
- **`parent`** — rows whose parent column equals a conversation's id are its children, recursively (to
  depth 16, never revisiting a row); siblings are ordered by id.
- **`tokens`** — each counter is a column name or columns joined by `+` / `-`, like a `jsonl`
  counter expression. **`cost`**, **`model`** and **`name`** (a child's label, default its id) are
  columns. `turns` is not a column of such a store and reads 0.

The store is opened **read-only** with a 200 ms busy timeout, so a running agent's writer is never
contended. A store that cannot be read now (busy, locked, malformed) yields **no rows**, so the
stream keeps its last snapshot instead of showing zeros; a store or root row that does not exist yet
yields a zero main row, as a missing transcript does. A row without a usable id is skipped and logged.

A spec with an unknown format, an unknown template variable, an empty `sources`, an empty field path,
a leftover `transcript_usage` key, a malformed counter expression, `children_by` or `main.match`
without an `identity`, an `identity` beside path-located `children`, or **two sources mapping the same counter** (or both mapping
`cost`) is **refused at load**, naming the file. When two sources of one binding describe the same
conversation, their rows **merge into one**: each counter and the cost come from the one source that
maps it; model and turns from the first source that has them. A session whose agent
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
- **Main `codex` / `codex-acp` agent** — `codex.yaml` and `codex-acp.yaml` bind Codex's rollout files,
  `{agent_home}/.codex/sessions/*/*/*/rollout-*.jsonl`, one per thread. The first line
  (`session_meta`) carries the thread id (`payload.id`), the `cwd` and, for a spawned thread,
  `payload.parent_thread_id`. The main conversation is the **newest rollout with no parent whose `cwd`
  is the session's worktree** (the daemon does not mint Codex's thread id; a spawned thread shares its
  parent's `cwd` and is never taken as the main one). Usage is the last `token_count` event's
  cumulative `total_token_usage` (`aggregate: last`): `input` = `input_tokens − cached_input_tokens`,
  `cache_read` = `cached_input_tokens`, `cache_write` = `cache_write_input_tokens`,
  `output` = `output_tokens − reasoning_output_tokens`, `reasoning` = `reasoning_output_tokens`; the
  model is the last `turn_context` one. **Spawned threads** are nested under the thread that spawned
  them, to any depth.
- **Main `opencode` agent** — `opencode.yaml` binds OpenCode's `opencode.db`,
  `{agent_home}/.local/share/opencode/opencode.db`, table `session_v2`, one row per session carrying
  running totals. The main conversation is the **newest root session whose `directory` is the
  session's worktree** (OpenCode keeps one conversation per worktree); sessions it spawned are nested
  by `parent_id`, to any depth. Counters: `tokens_input` → input, `tokens_cache_read` → cache read,
  `tokens_cache_write` → cache write, `tokens_output` → output, `tokens_reasoning` → reasoning;
  `cost`, `model` and `agent` (the child's name) come from their columns.
- **Usage tapped from the live stream.** An agent whose usage exists only in its protocol stream
  declares the events that carry it — `headless.usage_events` (stream-json, codex-jsonl and every
  other headless CLI) or `acp.usage_events` (ACP `session/update`s) — as `{ field, equals }`
  predicates. The driver appends each matching event, **verbatim**, to
  `<session_dir>/usage/<conversation_id>.jsonl`, and the agent's binding reads that file with the
  ordinary `jsonl` format, so no driver names a usage field. A write failure is logged and switches
  the tap off for the turn; it never fails the turn. `claude.yaml` taps its headless `result` and
  binds its **cost** (`total_cost_usd`) as a second source; the tokens stay with the transcript.
- **Coding subagents the daemon runs for a session** (opened through `SessionAgentService`) — each
  conversation is registered in `<session_dir>/usage-conversations.jsonl` (spec id, conversation id,
  agent home, worktree, parent = the session's main conversation, label = the subagent's name; one
  appended line per open, the last per conversation id winning on a resume). The usage stream
  evaluates **that agent's** binding for it and nests its row under the main conversation. A session
  with no recorded main conversation registers nothing. The subagent's
  `SubagentSession::cumulative_usage` reads the same row, so `subagent_list` agrees with the Usage
  tab; its `context_tokens` stays 0.
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
- `tddy-usage` — the binding model, its evaluation (`account`, `gather_session_usage`; the `sqlite`
  format reads through `rusqlite`) and
  `token_accounting` (`TokenUsage`, `ConversationRecord`, `format_token_summary`); it names no agent.
  `tddy-agent-backend::token_accounting` re-exports it.
- `tddy-coding-agent-specs` — parses and validates `coding_agent.usage`, `headless.usage_events` and
  `acp.usage_events`; the Claude bindings in `specs/claude.yaml` and `specs/claude-acp.yaml`, the
  Codex bindings in `specs/codex.yaml` and `specs/codex-acp.yaml`, the OpenCode binding in
  `specs/opencode.yaml`.
- `tddy-agent-backend::usage_tap` — `UsageTapSpec` (`for_conversation`) and `UsageTap`, fed by the
  headless driver (each stdout line) and the ACP driver (each `session/update`), given through
  `HeadlessCommand::usage_tap` / `OpenSession::usage_tap`.
- `tddy-session-agents` — registers each coding subagent conversation it opens and attaches its
  `UsageReading` (`tddy-coding-subagent`); `tddy-session-lifecycle`'s headless launcher sets the tap.
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
- **Out of scope:** TUI visualization; USD cost estimation; bindings for agents other than Claude,
  Claude-ACP, Codex, Codex-ACP and OpenCode. Cursor's headless `result` carries no usage in any
  recorded stream, so Cursor declares no tap yet; ACP's `usage_update` cost is cumulative and is not bound
  yet — no recorded ACP stream exists to bind it against
  ([todo](../../dev/todo/2026-11-11-acp-usage-update-cost-unbound.md)). A `shared`-placement ACP subagent is registered but not yet tapped
  ([todo](../../dev/todo/2026-11-11-shared-acp-conversation-usage-untapped.md)).

> ⚠️ The live end-to-end stderr summary against a real Ollama-backed session has not yet been
> exercised; the behavior is covered by unit/acceptance tests (including against a real Claude
> transcript layout on disk).

> The Codex fixtures under test are synthetic: builders that write lines shaped like Codex CLI 0.167
> rollouts (header, `turn_context`, `token_count`). The binding has not been run against a real user's
> rollout directory, and `main.path`'s three-level date glob walks every rollout the user ever
> wrote.

> The OpenCode fixtures are synthetic rows in a store built with the real `session_v2` schema. The
> only local `opencode.db` held no sessions, so whether `tokens_output` already includes
> `tokens_reasoning` is unverified; the binding maps them as disjoint columns
> ([todo](../../dev/todo/2026-11-11-opencode-reasoning-overlap-unverified.md)).
