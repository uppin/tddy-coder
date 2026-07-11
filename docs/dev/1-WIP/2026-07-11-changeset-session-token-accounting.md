# Changeset: session-token-accounting — per-conversation token accounting + conversation listing

**Date:** 2026-07-11
**Branch:** `session-token-accounting`
**Packages:** `tddy-discovery`, `tddy-core`, `tddy-tools`, `tddy-sandbox-runner`, `tddy-sandbox-app`
**Feature PRD:** [docs/ft/coder/session-token-accounting.md](../../ft/coder/session-token-accounting.md)

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] `tddy-discovery`: `TokenUsage`, `usage` on `ChatCompletionResponse`, per-turn +
      cumulative usage on subagent sessions (`openai.rs`, `subagent.rs`)
- [x] `tddy-core`: `token_accounting` module — canonical `TokenUsage`, `ConversationRecord`,
      `format_token_summary`, `read_main_agent_usage` (transcript JSONL reader)
- [x] `tddy-tools`: `SubagentConversation` record, `subagent_list` MCP tool, usage in
      `subagent_prompt` result JSON, accounting-file writer (`TDDY_TOOLS_ACCOUNTING_FILE`)
- [x] `tddy-sandbox-runner`: set `TDDY_TOOLS_ACCOUNTING_FILE` on the `tddy-tools --mcp` spawn
- [x] `tddy-sandbox-app`: read accounting file + main-agent transcript, print summary at exit

## Acceptance tests

- [x] `packages/tddy-tools/tests/subagent_token_accounting_acceptance.rs`
- [x] `packages/tddy-core/tests/token_accounting_acceptance.rs`
- [x] `packages/tddy-core/tests/claude_transcript_usage_acceptance.rs`

## Unit tests

- [x] `packages/tddy-discovery/tests/subagent_usage_red.rs`
- [x] `packages/tddy-core/tests/token_accounting_red.rs`

## Validation Results

### pr-wrap (2026-07-11)

**Status:** ✅ ready for PR.

- **Tests:** 12/12 new tests pass (discovery 4, core unit 3 + acceptance 2, tools acceptance 3);
  no regressions (tddy-discovery + tddy-core libs: 244 + 23 pass, 0 failed).
- **Clippy:** `cargo clippy -p tddy-discovery -p tddy-core -p tddy-tools -p tddy-sandbox-runner
  -p tddy-sandbox-app --all-targets -- -D warnings` → clean (exit 0).
- **Build:** both binaries compile (covered by the `--all-targets` clippy pass).
- **Change risk:** additive; token accounting is best-effort telemetry (write/read failures
  ignored, never load-bearing) — matches the plan's intent, no fallbacks in critical paths.
- **Test quality:** fluent Given/When/Then, exact-equality asserts, real wiremock/tempfile
  fakes, acceptance tests drive the real `tddy-tools --mcp` wire. No mocks-of-everything.
- **Prod readiness:** no mock/stub code, no TODO/FIXME, no test-only branches (`is_claude_agent`
  is a legitimate runtime distinction for Cursor sessions, not test detection).
- **Clean code:** small documented fns; `conversation_records` is the single DRY source for
  `subagent_list` + the accounting file; canonical `ConversationRecord` reused across crates.

**Note — formatting:** `cargo fmt` was run on `tddy-discovery`, `tddy-core`, `tddy-sandbox-app`.
It was **not** run workspace-wide because `tddy-tools/src/server.rs` and
`tddy-sandbox-runner/src/runner.rs` are stored single-line/minified in the repo; a blanket
`cargo fmt` would reflow those entire files (huge unrelated diff). Insertions there are
rustfmt-style and compile clean under clippy.

## Delta summary

### `tddy-discovery`

- `openai.rs`: add `TokenUsage { input_tokens, output_tokens }` (`total()`, field-wise add);
  add optional `usage` to `ChatCompletionResponse` deserialized from
  `usage {prompt_tokens → input, completion_tokens → output}`. Absent/partial usage → zeros.
- `subagent.rs`: `PromptOutcome` gains `usage: TokenUsage` (per-`prompt()` tokens);
  `send_turn_and_check_final_answer` returns each turn's usage; `FastContextSession` and
  `SpecializedSubagentSession` store `model` + a running usage accumulator; the
  `SubagentSession` trait gains `fn model(&self) -> &str` and
  `fn cumulative_usage(&self) -> TokenUsage`.

### `tddy-core`

- New agent-neutral `token_accounting` module: canonical `TokenUsage`; `ConversationRecord`
  `{ agent, id, model, input_tokens, output_tokens, total_tokens, turns }`
  (serde camelCase: `inputTokens`/`outputTokens`/`totalTokens`) shared by the accounting
  file, the `subagent_list` output, and the summary; `format_token_summary(session_id,
  records) -> String` (per-record lines + TOTAL row).
- `backend/claude.rs`: the Claude-Code-specific transcript readers live with the Claude backend
  (not the generic module), re-exported via `backend/mod.rs`:
  - `read_claude_transcript_usage(...)` → main-thread `ConversationRecord` from
    `<home>/.claude/projects/*/<session_id>.jsonl` (assistant `message.usage`, input/output only,
    `cache_*` excluded); missing transcript → zeros + fallback model.
  - `read_claude_subagent_usages(...)` → one `ConversationRecord` per nested Task-tool subagent
    from `<project>/<session_id>/subagents/agent-*.jsonl` (agent name from the sibling
    `.meta.json` `agentType`, id from the file stem), sorted by id.
  - shared `sum_assistant_usage` helper folds a transcript's assistant `message.usage`.

### `tddy-tools`

- `server.rs`: session table value becomes `SubagentConversation { agent, id, model, turns,
  usage, session }`; set agent/model at `subagent_new_session`, fold per-turn usage + bump
  turns at `subagent_prompt`. `prompt_outcome_json` gains a `usage` object. New MCP tool
  `subagent_list` → `{ conversations: [ConversationRecord…] }`. On each prompt/cancel,
  overwrite `TDDY_TOOLS_ACCOUNTING_FILE` (when set) with `{ conversations: […] }`.

### `tddy-sandbox-runner`

- `runner.rs`: set `TDDY_TOOLS_ACCOUNTING_FILE = egress_log_path(egress_dir,
  "accounting.json")` on the in-jail `tddy-tools --mcp` spawn, mirroring `TDDY_TOOLS_LOG_FILE`.

### `tddy-sandbox-app`

- `main.rs`: after the terminal bridge returns, read
  `<session_dir>/egress/accounting.json` (tddy subagent conversations) + call
  `tddy_core::backend::{read_claude_transcript_usage, read_claude_subagent_usages}` for the main
  Claude agent and its nested Task subagents, then print `format_token_summary` to stderr.
