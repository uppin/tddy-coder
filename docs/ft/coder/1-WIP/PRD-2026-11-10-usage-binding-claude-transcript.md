# Declarative usage bindings, with Claude's transcripts as the first - PRD

**Date**: 2026-11-10
**PRD Type**: Technical Improvement
**Stack**: `#usage-binding 1/5` — branch `feature/usage-binding/claude-transcript`

## Affected Features

- **Primary Feature**: [Session Token Accounting](../session-token-accounting.md) — the conversations
  accounted, the counters per conversation, and where an agent's usage source is declared.
- **Related Feature**: [Session Usage Inspector](../../web/session-usage-inspector.md) — the record it
  renders gains cache-read, cache-write, cost and parent fields (rendered by `#usage-binding 2/5`).

## Summary

Token accounting stops being Claude code. Each coding agent spec declares, under
`extensions.coding_agent.usage`, **where its usage lives and how to read it**: a format, path
templates, a record filter, a field mapping, a dedupe key and an aggregation. One agent-agnostic engine
in a new crate, `tddy-usage`, evaluates any such binding. Claude's transcript becomes the first binding.
It is declared in `claude.yaml` and `claude-acp.yaml`, and the Rust Claude reader is deleted.

The Claude numbers also become correct. Two problems are fixed along the way:
- **Duplicate lines are counted twice.** Claude Code writes one transcript line per content block, all
  carrying the same `message.id` and usage. On a real session, 10 assistant lines had only 6 distinct
  ids, so "Out" was about 1.8× too high.
- **Cache traffic is missing.** Cache-read and cache-write tokens are accounted for the first time. On
  that session they were 332 059 and 43 803 tokens, against 12 fresh input tokens.

## Background

- Usage is gathered by one function, `gather_session_usage`.
- Its two callers gate on the agent:
  - the presenter's watcher compares `agent == "claude"`;
  - `tddy-sandbox-app` reads `coding_agent.transcript_usage: bool`.
- The reader is Claude-shaped Rust. It covers one level of subagents and excludes cache counters.
- No other agent can be accounted without more agent-specific Rust. That contradicts the rule the
  `#agent-tools` sweep established: agent metadata lives only in `specs/*.yaml`.

## Proposed Changes

### What's Changing

- **A `usage:` section in the coding-agent spec** (`extensions.coding_agent.usage`). It holds a list of
  `sources`, each naming a **format** from a closed vocabulary (this PR: `jsonl`), and the binding
  for it:
  - **`main.path`** — a path template naming the conversation's own file(s). Every match is merged.
    Variables: `{agent_home}`, `{session_dir}`, `{conversation_id}`, `{worktree}`.
  - **`children`** (optional) — a path template evaluated against each conversation found, using that
    file's `{dir}` and `{stem}`. Its matches are that conversation's children, and the template is
    applied to them in turn, **recursively to any depth**. A child's `id` and display `name` are
    templates too. The name may be read from a sidecar JSON file's field, with a default.
  - **`record`**:
    - `where` — field-equality predicates over a JSON line;
    - `dedupe_by` — a field path; the last line per key wins;
    - `aggregate` — `sum` over the kept records;
    - `model` — a field path; the last value seen;
    - `tokens` — a field path per counter: `input`, `cache_read`, `cache_write`, `output`, `reasoning`.
      The counters are disjoint: `input` is fresh input only, and `output` excludes reasoning;
    - `cost` — optional, a field path.
  - A record's `turns` is the number of distinct kept records.
- **Refused at load, naming the spec file**:
  - an unknown format word;
  - an unknown variable in a template;
  - a source with no `main.path`;
  - a counter mapped twice.
- **`coding_agent.transcript_usage` is removed.** An operator override that still carries it is refused
  at load (the section is `deny_unknown_fields`), with the file named. There is no fallback.
- **The conversation record gains fields**:
  - `cache_read_tokens`, `cache_write_tokens` and `reasoning_tokens`;
  - an optional `cost_usd`;
  - an optional `parent_id`, which places a child under its parent.

  `total_tokens` is `input + cache_read + cache_write + output + reasoning`. The counters are disjoint
  by definition, so nothing is counted twice. Every new field is serde-defaulted, so
  an `accounting.json` written by an older `tddy-tools` still reads.
- **Claude's binding** in `claude.yaml` and `claude-acp.yaml`:
  - **Main transcript:** `{agent_home}/.claude/projects/*/{conversation_id}.jsonl`.
  - **Children:** `{dir}/{stem}/subagents/agent-*.jsonl`, named from `{dir}/{stem}.meta.json`
    `agentType`.
  - **Filter:** keep lines where `type == assistant`.
  - **Dedupe key:** `message.id`.
  - **Model:** `message.model`.
  - **Counters:** `message.usage.{input_tokens, cache_read_input_tokens, cache_creation_input_tokens,
    output_tokens}`.
- **Both consumers take the binding as data.** The presenter's watcher and `tddy-sandbox-app`'s
  end-of-session summary receive it with `agent_home` and `conversation_id`. Neither names an agent any
  more. A session whose spec declares no `usage:` has no main-agent row. The tddy subagent rows from
  `egress/accounting.json` are still appended, as today.
- **The stderr summary** prints the cache counters per row and in the TOTAL row.

### What's Staying the Same

- The `accounting.json` file, its writer (`tddy-tools --mcp`) and its `{"conversations":[...]}` shape.
  It only gains optional fields.
- The presenter's polling cadence and "emit only on change" behaviour.
- Where the watcher runs (tddy-coder workflow sessions). Moving it to the daemon is `#usage-binding 2/5`.
- The web Usage tab's columns, which `#usage-binding 2/5` changes.

## Impact Analysis

### Technical Impact

- **New crate `tddy-usage`.** It holds the binding model (serde), template substitution, the JSONL
  format and the conversation tree, and it names no agent; a source-scan test asserts this.
- **`tddy-coding-agent-specs`** parses and validates `usage:` into `tddy-usage` types.
- **`tddy-agent-backend`**:
  - `gather_session_usage` takes bindings instead of `include_main_agent` / `claude_home`;
  - `backend/claude/transcript_usage.rs` is deleted;
  - `token_accounting::ConversationRecord` gains the new fields.
- **`tddy-presenter`**: the `usage_watcher` config loses `agent: String` and `claude_home`, and gains the
  bindings, `agent_home` and `conversation_id`.
- **`tddy-coder`** (the watcher call site) and **`tddy-sandbox-app`** pass the resolved spec's binding.
- **The proto `ConversationRecord`** gains the fields, and `convert.rs` maps them. The web keeps
  ignoring them until 2/5.

### User Impact

- Claude usage numbers change, and become correct: lower Out, and cache counters appear in the stderr
  summary.
- **Breaking:** a `<tddyhome>/tools/claude.yaml` override with `transcript_usage:` fails to load until
  the line is replaced by a `usage:` section.

## Implementation Plan

1. `tddy-usage`: the binding model, templates, the JSONL format, recursive children, dedupe, sum.
2. The specs section and its load-time refusals, plus Claude's binding.
3. Swap `gather_session_usage` and both consumers onto bindings, and delete the Claude reader.
4. Extend the record, the proto, `convert.rs` and the stderr summary.
5. Tests:
   - engine unit tests;
   - acceptance tests over transcript fixtures cut from a real session (duplicate ids, cache counters,
     nested subagents two deep);
   - spec-load refusals;
   - the presenter watcher driven by a binding.

## Acceptance Criteria

- [ ] A Claude transcript whose assistant lines repeat a `message.id` is counted once per id.
- [ ] Cache-read and cache-write tokens are reported per conversation and in the totals.
- [ ] Subagents are found recursively — a subagent of a subagent is a row whose `parent_id` is its
      parent.
- [ ] `claude.yaml` / `claude-acp.yaml` declare the binding; no Rust outside YAML names an agent to
      account usage, and `tddy-usage` passes a no-agent-vocabulary scan.
- [ ] A spec with an unknown usage format, an unknown template variable or a leftover
      `transcript_usage` is refused at load, naming the file.
- [ ] A session whose agent declares no `usage:` reports only its tddy subagent rows.
- [ ] `accounting.json` from an older writer (no new fields) still reads.

## References

### Affected Features (Complete List)
- [Session Token Accounting](../session-token-accounting.md) — the binding model, the Claude binding,
  the cache counters and recursion.
- [Session Usage Inspector](../../web/session-usage-inspector.md) — record shape only.

### Related Documentation
- `packages/tddy-coding-agent-specs/docs/coding-agent-specs.md` — "the only home of agent metadata".
- Successors: `#usage-binding 2/5` (daemon stream + Usage tab), `3/5` (live tap), `4/5` (Codex),
  `5/5` (OpenCode).
