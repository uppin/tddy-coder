# Validate Tests Report — Codex CLI Backend

**Date:** 2026-03-29

## Context

Refactor validation for the Codex CLI backend feature (see `plan/evaluation-report.md`): `CodexBackend`, JSONL streaming/parser, CLI/config wiring, stub-based integration tests. Evaluation flagged medium risk around subprocess argv contract, exit-code semantics vs Cursor, and cleanup of stray test artifacts.

## Commands Run

From repo root, via nix dev shell (`./dev`):

1. **Primary**

   ```bash
   ./dev cargo test -p tddy-core --lib -p tddy-coder --tests -p tddy-integration-tests --test codex_backend 2>&1
   ```

   - **Exit code:** `0`

2. **Supplemental (name filter)**

   ```bash
   ./dev cargo test codex --no-fail-fast 2>&1
   ```

   - **Exit code:** `0`

### Note on primary invocation scope

With this flag ordering, Cargo executed **all** integration test binaries in `tddy-integration-tests`, not only `codex_backend` (the `--test codex_backend` filter did not restrict that package to a single binary in this multi-`-p` invocation). The run therefore exercised the full integration suite for that package plus `tddy-coder` integration tests and `tddy-core` library tests.

## Pass / Fail Summary

| Command | Failed tests | Ignored (where reported) |
|--------|----------------|---------------------------|
| Primary | **0** | 6 total across targets (e.g. changeset, cursor, red integration binaries) |
| `codex` filter | **0** | (filtered runs mostly show 0 ignored on matching targets) |

**Failures:** none (no failing test names).

**Approximate primary run totals** (sum of per-target `test result: ok. N passed` lines): **~460** tests executed, **0** failed.

**Codex-focused integration tests** (`tddy-integration-tests`, `codex_backend.rs`, unix): **6** passed:

- `codex_backend_spawns_exec_with_json_and_prompt`
- `codex_backend_resume_subcommand_includes_session_id`
- `codex_backend_merges_system_prompt_like_cursor`
- `codex_backend_includes_model_flag_when_set`
- `codex_backend_propagates_exit_code`
- `codex_backend_reports_binary_not_found`

**`tddy-core` library** (primary run): **98** passed, including Codex argv/prompt/selection/parser unit tests.

**`cargo test codex`:** matched **14** tests in `tddy-core` lib (codex backend/stream/parser/selection), plus **`cli_accepts_agent_codex`** in `tddy-coder` `cli_args` (other workspace crates had 0 matching tests in the filter).

**Full workspace test suite:** not run as a single `cargo test` without filter; only the primary multi-package command and the `codex` filter run were executed.

## Coverage Gaps / Missing Tests

- **ProgressSink:** Evaluation notes JSONL lines do not yet drive `ProgressSink`; no tests asserting progress events from streamed Codex output.
- **Real Codex CLI:** All coverage uses **shell stubs** that record argv and emit fixture JSONL; there is no opt-in smoke test against an installed real `codex` binary (would be environment-dependent; document if intentionally out of scope).
- **Exit / status semantics:** Confirm and test workflow behavior when `InvokeResponse` is `Ok` but carries a **non-zero** `exit_code` (alignment with Cursor backend and user expectations).
- **Streaming edge cases:** Limited coverage for partial lines, interleaved stderr, very large JSONL payloads, or unexpected event types (beyond existing malformed-line parser test).
- **Sandbox / approval argv mapping:** Documented in argv builder per evaluation; dedicated tests for every flag combination may still be thin.
- **Repository hygiene:** `.codex-red-test-output.txt` is untracked and should not ship; not a test gap but a validation/process note from the evaluation report.

## Recommendations

1. Add unit or integration tests that feed representative JSONL (including progress-like events) and assert **`ProgressSink`** callbacks when that API is wired to Codex streaming.
2. Add a **contract test** (or explicit doc + test) for **non-zero exit_code with Ok** invoke result so behavior matches Cursor and the workflow state machine.
3. If CI should stay hermetic, keep stubs as default; optionally add a **`CODEX_CLI_SMOKE`** (or similar) **ignored** or **explicitly gated** test that runs only when a real binary path is set, so developers can validate argv/output against a live CLI without flaking default CI.
4. For faster, narrower validation of this feature alone, prefer explicit invocations such as `./dev cargo test -p tddy-core --lib` and `./dev cargo test -p tddy-integration-tests --test codex_backend` **separately** instead of relying on one multi-`-p` line to mean “only codex_backend integration.”
5. Delete or gitignore **`.codex-red-test-output.txt`** before merge.
