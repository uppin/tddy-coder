# 2026-11-10 — Declarative usage bindings, with Claude's transcripts as the first

`#usage-binding 1/5`, PR [#890](https://github.com/uppin/tddy-coder/pull/890), stack
[#895](https://github.com/uppin/tddy-coder/stack/895). Feature:
[session-token-accounting.md](../../ft/coder/session-token-accounting.md).

## What changed

- **`tddy-usage` (new crate)** — the binding model, path templates, field paths, the `jsonl` format
  (filter, dedupe by key, sum, last model, turns), the recursive conversation tree (depth 16, cycle
  guard, siblings by id), `account`, `gather_session_usage`, and `token_accounting` moved from
  `tddy-agent-backend` with `cacheReadTokens`, `cacheWriteTokens`, `reasoningTokens`, `costUsd?`,
  `parentId?` (serde-defaulted) and `ConversationRecord::new`. It names no agent (source scan).
  [usage-bindings.md](../../../packages/tddy-usage/docs/usage-bindings.md).
- **`tddy-coding-agent-specs`** — `coding_agent.usage`, validated at load; `transcript_usage`
  removed and refused; the Claude bindings in `claude.yaml` and `claude-acp.yaml`.
- **`tddy-agent-backend`** — `backend/claude/transcript_usage.rs`, its re-exports and
  `backend::gather_session_usage` deleted; `token_accounting` re-exports `tddy-usage`'s.
- **`tddy-presenter`** — the session usage watcher takes the binding and a `UsageContext`; the
  `agent == "claude"` check is gone. **`tddy-coder`** passes
  `CodingAgentRegistry::builtin_cached().get(agent).usage`. **`tddy-sandbox-app`** passes the spec's
  `usage` to `print_token_summary`.
- **`tddy-service`** — proto `ConversationRecord` fields 8–12 and their `convert.rs` mapping;
  `remote_pb.ts` regenerated in `tddy-client-web` and `tddy-rust-typescript-tests`.
- **`tddy-discovery`** — `conversation_record` uses `ConversationRecord::new`.

## Deviations from the plan

- **No `spec/usage.rs`.** `parse_section` calls `tddy_usage::validate_usage_section` directly; a
  wrapper would only have forwarded.
- **`print_token_summary(session_id, usage, ctx)`** in `tddy-sandbox-app`, not
  `(session_dir, usage, ctx)`: the session dir is in `ctx`, and the summary heading needs the id.
- **`TokenUsage` is not widened** (about 15 literal sites outside this node); only
  `ConversationRecord` gains the counters. `ConversationRecord` drops `Eq` (`cost_usd: f64`).
- **Fixtures are builders** (`tests/support/mod.rs`) writing lines as Claude Code does, not a file cut
  from a transcript; the measured session's numbers are reused in the cache test.
- `packages/tddy-workflow-recipes/generated/schema-manifest.json` is committed as build output.

## Measured

Session `01a1c71c-…`: 10 assistant lines, 6 `message.id`s — naive Out 3648 against 2057
deduplicated; fresh input 12 against cache read 332 059 and cache write 43 803.

## Code issues

- `tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md` — **closed**: `backend/mod.rs`
  537 → **485** production lines (budget 500) once `gather_session_usage` and the Claude re-exports
  left. Record deleted.
- `tddy-coder` `oversized-file-run` — 2,501 → 2,511, **regressed, split deferred**: stack-shared with
  `#usage-binding 2/5`.
- `tddy-discovery` `oversized-file-subagent-runtime` — 823 → 823, unchanged.
- `tddy-sandbox-app` `oversized-file-main` — 966 → 973, **regressed**. The engine split (`move_item` into
  `token_summary`) passed `check --deep` but `apply` declared the module in `lib.rs` instead of the
  binary root and failed to compile; rolled back, recorded, not moved by hand.

## Wrap clean-ups

`/pr-wrap` extracted `passes_where`, `keep` and `sum` from `jsonl::fold_files` and `in_sibling_order`
from the tree walk (one sort and flatten instead of three), added `ConversationRecord::new` for the
fresh-input-only rows, and corrected stale `transcript_usage` / `gather_session_usage` references in
docs and the presenter test header.

## Left for later nodes

Cursor binding, ACP cost and the OpenCode reasoning overlap belong to the stack's sweep node; the
presenter's snapshot-on-connect TODO is pre-existing and goes with the watcher's move in 2/5.
