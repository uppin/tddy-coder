# Changeset: OpenAI Codex CLI backend

**Date**: 2026-03-29  
**Status**: Complete  
**Type**: Feature

## Affected Packages

- `tddy-core`
- `tddy-coder`
- `tddy-integration-tests`

## Related Feature Documentation

- [Coder overview](../../ft/coder/1-OVERVIEW.md)
- [Planning step](../../ft/coder/planning-step.md)
- [Coder changelog](../../ft/coder/changelog.md)

## Summary

`tddy-core` exposes `CodexBackend`: non-interactive `codex exec` and `codex exec resume <id>` with `--json` JSONL on stdout. `tddy-coder` accepts `--agent codex`, optional `--codex-cli-path`, and environment variable `TDDY_CODEX_CLI` for the binary. Backend selection includes Codex between Cursor and Stub; default model key for codex is `gpt-5`.

## Technical State (reference for wrap)

### `tddy-core`

- `CodingBackend` implementation `CodexBackend` in `src/backend/codex.rs`.
- Prompt merge matches Cursor precedence (`system_prompt_path` over inline `system_prompt`, then user prompt with blank-line separation).
- `build_codex_exec_argv` emits `exec`, optional `resume` + session id, `--json`, optional `-C` and `-m`, `--sandbox` + `--ask-for-approval never` mapped from `GoalHints` (plan read-only vs editing goals).
- `src/stream/codex.rs` parses JSONL for `session` / `session_id` and `item.completed` text; malformed lines surface in parse output when no successful fields exist.
- `InvokeResponse` carries subprocess `exit_code` (including non-zero) as `Ok` with populated `raw_stream` / `stderr` when present; `BinaryNotFound` when the executable is missing.

### `tddy-coder`

- `create_backend` resolves Codex via `resolve_codex_binary` (CLI path, then `TDDY_CODEX_CLI`, then `codex` on `PATH`).
- `verify_tddy_tools_available` applies to codex the same as claude and cursor (stub and claude-acp exempt).
- YAML `codex_cli_path` merges from config like `cursor_agent_path`.

### Tests

- Unit tests in `codex.rs` and `stream/codex.rs`; integration stub tests in `packages/tddy-integration-tests/tests/codex_backend.rs`.

## Follow-ups (optional)

- Map Codex JSONL event types to `ProgressSink` / `ProgressEvent` for live progress.
- Workflow layers should treat `InvokeResponse.exit_code` when the backend returns `Ok`.
