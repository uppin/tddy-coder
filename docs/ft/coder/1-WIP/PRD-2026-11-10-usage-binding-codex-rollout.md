# Codex usage from its rollout files - PRD

**Date**: 2026-11-10
**PRD Type**: Enhancement
**Stack**: `#usage-binding 4/5` — branch `feature/usage-binding/codex-rollout`, on `#usage-binding 3/5`

## Affected Features

- **Primary Feature**: [Session Token Accounting](../session-token-accounting.md) — Codex
  conversations, and the `jsonl` format features they need.

## Summary

Codex records every thread in `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<thread_id>.jsonl`.
- **Header:** the first line is `session_meta`, carrying the thread `id`, the `cwd` and, for a spawned
  thread, `parent_thread_id` (top level of the payload, verified on Codex CLI 0.167).
- **Usage:** each `event_msg` of type `token_count` holds `info.total_token_usage`. This is a
  **cumulative** total: `input_tokens`, `cached_input_tokens` (part of the input), `output_tokens` and
  `reasoning_output_tokens`.

`codex.yaml` and `codex-acp.yaml` bind these files. The generic `jsonl` format gains what the binding
needs, and no Codex code:

- **`aggregate: last`** — a cumulative counter takes the last kept record, not the sum.
- **Identity from the header** — a source may read a file's conversation id and parent id from fields
  of its first matching record, instead of from the path.
- **Children by parent field** — every file under the root whose parent id equals a conversation's id
  is that conversation's child, recursively.
- **Root match by field** — the main conversation is the file whose header field equals a template.
  For Codex that is the `cwd` matched against `{worktree}`, the newest matching file, because the
  daemon does not mint Codex's thread id.
- **Subtraction in a mapping** — `input: "a - b"` maps fresh input as total input minus the cached part.

## Background

Codex usage is only debug-logged today (`stream/codex.rs`). The rollout files are the durable source,
and they keep the parent links for spawned threads.

## Proposed Changes

### What's Changing
- The `jsonl` format gains:
  - `aggregate: last`;
  - header identity (`identity: { id, parent }` field paths);
  - children by parent field;
  - a main-file match by field with a newest-wins rule;
  - `-` in counter expressions.
- Each feature is refused at load when misused. An unknown field operator is one such case.
- Codex bindings in `codex.yaml` and `codex-acp.yaml`:
  - **Root:** `{agent_home}/.codex/sessions/**/rollout-*.jsonl`.
  - **Filter:** keep `payload.type == token_count`.
  - **Aggregation:** `last`.
  - **Counters:** `input` = `…input_tokens - …cached_input_tokens`, `cache_read` =
    `…cached_input_tokens`, `output` = `…output_tokens - …reasoning_output_tokens`, `reasoning` =
    `…reasoning_output_tokens`. OpenAI counts reasoning inside output, and the record keeps the counters
    disjoint; this node verifies that against its fixtures.
  - **Model:** from `turn_context.payload.model`.

### What's Staying the Same
- The Claude binding and every earlier node's behaviour.

## Impact Analysis

### Technical Impact
- `tddy-usage` (the jsonl format features) and the specs (the Codex bindings and validation).

### User Impact
- Codex sessions and their spawned threads show real usage, nested.

## Implementation Plan
1. The format features, each with unit tests.
2. The Codex bindings.
3. Acceptance tests over rollout fixtures cut from real files: a parent thread with a spawned child,
   cumulative counters, cached input subtracted.

## Acceptance Criteria
- [ ] A Codex thread reports its last cumulative `token_count`, with fresh input = input − cached.
- [ ] A spawned thread appears as a child of the thread that spawned it, recursively.
- [ ] The main Codex conversation is found by its `cwd`.
- [ ] No Rust names Codex; the bindings are YAML.

## References
### Affected Features (Complete List)
- [Session Token Accounting](../session-token-accounting.md)
### Related Documentation
- Parent: `#usage-binding 3/5`.
