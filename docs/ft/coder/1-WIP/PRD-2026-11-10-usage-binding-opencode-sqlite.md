# OpenCode usage from its SQLite store - PRD

**Date**: 2026-11-10
**PRD Type**: Enhancement
**Stack**: `#usage-binding 5/5` — branch `feature/usage-binding/opencode-sqlite`, on `#usage-binding 4/5`

## Affected Features

- **Primary Feature**: [Session Token Accounting](../session-token-accounting.md) — OpenCode
  conversations, and the new `sqlite` usage format.

## Summary

OpenCode keeps one row per session in `opencode.db`, table `session_v2`. Each row carries:
- running totals: `tokens_input`, `tokens_output`, `tokens_reasoning`, `tokens_cache_read`,
  `tokens_cache_write`;
- `cost`;
- the agent's `model` and `agent`;
- the session's `directory`;
- the `parent_id` of a session spawned by another.

A new generic **`sqlite` format** reads such a store, and `opencode.yaml` binds it. The format holds no
SQL beyond what the binding names:
- the database path template;
- a table;
- the id and parent-id columns;
- a column per counter;
- the cost and model columns;
- a root match (`column = template`).

The database is opened **read-only**, so a running agent's writer lock is never contended. A busy
database yields the last snapshot, not an error row.

## Background

- OpenCode in a jail uses a per-session home (`<session_dir>/sandbox/.work/home`), so its store lands
  at `.local/share/opencode/opencode.db` under it.
- A host OpenCode uses the OS user's home.
- OpenCode keeps one conversation per worktree (`--continue`), so the root session is the newest root
  row whose `directory` is the session's worktree.

## Proposed Changes

### What's Changing
- **The `sqlite` format in `tddy-usage`**, with these binding fields:
  - `path`;
  - `table`;
  - `id`;
  - `parent`;
  - `root: { column, equals, newest_by }`;
  - `tokens: { input, cache_read, cache_write, output, reasoning }` (each an expression over column
    names, `+` and `-` allowed);
  - `cost`;
  - `model`;
  - `name`.

  Children are rows whose parent column equals a conversation's id, recursively. Column and table names
  are validated as identifiers at load.
- **An OpenCode binding** in `opencode.yaml`:
  - **Store:** `{agent_home}/.local/share/opencode/opencode.db`.
  - **Table:** `session_v2`.
  - **Root:** the newest row whose `directory` = `{worktree}`, matched on `time_updated`.
  - **Counters:** `tokens_*`, with `tokens_reasoning` mapped to `reasoning`. Whether `tokens_output`
    already includes reasoning is checked against a real store in this node, and subtracted if it does.
  - **Cost:** `cost`.

### What's Staying the Same
- The `jsonl` format and every other binding.

## Impact Analysis

### Technical Impact
- `tddy-usage` gains `rusqlite`, already a workspace dependency (`tddy-semantic-index`, 0.32 bundled),
  so it adds no new external dependency.
- The specs gain the OpenCode binding and `sqlite` validation.

### User Impact
- OpenCode sessions show per-session and per-child usage with cost.

## Implementation Plan
1. The `sqlite` format: read-only open, identifier validation, recursion.
2. The OpenCode binding.
3. Tests against a fixture database built with the real `session_v2` schema: a root with a child and a
   grandchild, cost, a busy or locked database.

## Acceptance Criteria
- [ ] An OpenCode session's root conversation and its spawned children show tokens, cache and cost.
- [ ] The database is opened read-only; a locked database does not fail the snapshot.
- [ ] A binding naming a non-identifier column or table is refused at load.
- [ ] No Rust names OpenCode; the binding is YAML.

## References
### Affected Features (Complete List)
- [Session Token Accounting](../session-token-accounting.md)
### Related Documentation
- Parent: `#usage-binding 4/5`.
