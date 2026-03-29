# Clean-code analysis: Codex CLI backend

**Scope:** `backend/codex.rs`, `stream/codex.rs`, `tddy-coder` Codex wiring in `run.rs`, integration tests `codex_backend.rs`. **Context:** `plan/evaluation-report.md` (medium risk; argv contract, exit-code semantics vs Cursor, ProgressSink gap).

---

## Naming consistency (vs Cursor / Claude)

- **Types and API surface** align with existing backends: `CodexBackend`, `DEFAULT_CLI_BINARY`, `with_path`, `CodingBackend::name()` → `"codex"` (parallel to `CursorBackend` / `"cursor"`).
- **Argv builders:** Cursor exposes `build_cursor_cli_args`; Codex exposes `build_codex_exec_argv`. Naming is parallel though not identical (`_cli_args` vs `_exec_argv`); both are `pub(crate)` for tests—consistent intent.
- **Prompt merge:** Codex factors merging into `merge_codex_prompt` and documents parity with `CursorBackend` in the doc comment. Cursor still inlines the same system/path + `format!("{}\n\n{}", …)` logic inside `invoke_sync`. That is **intentional duplication** today: Codex is slightly cleaner; extracting a shared helper would reduce drift but is out of current scope.
- **Logging prefixes:** Codex uses `[tddy-codex]` in `backend/codex.rs` and `stream/codex.rs`; Cursor uses `[tddy-coder] Cursor backend …` in places. Minor inconsistency for grepability; not functionally wrong.
- **CLI resolution:** `resolve_codex_binary` mirrors `resolve_cursor_agent_binary` (arg/config → env var → default binary). Env names `TDDY_CODEX_CLI` / `TDDY_CURSOR_AGENT` follow the same pattern.

---

## Function length and complexity

- **`invoke_sync` in `codex.rs` (~165 lines):** Large single function covering command build, spawn, stderr drain, optional conversation log, stdout line collection, wait, parse, and `InvokeResponse` assembly. **Same structural shape as `CursorBackend::invoke_sync`**, which is longer due to progress callbacks, stream parsing hooks, and resume skip logic. Codex is simpler but still a **god-method** candidate: splitting into “configure `Command`”, “drain streams”, “append conversation file” would improve testability without changing behavior.
- **`build_codex_exec_argv`:** Short, linear, easy to review—good.
- **`merge_codex_prompt`:** Short—good.
- **`parse_codex_jsonl_output`:** Single loop with a type match; complexity is O(n) over lines; appropriate size.

---

## Duplication vs Cursor invoke path

- **Shared patterns (not factored):** `which_binary` / `format_command_for_log`, `PATH` augmentation, `TDDY_*` env vars, piped stdout/stderr, stderr background read, conversation JSON prelude, spawn error mapping to `BinaryNotFound`, `set_child_pid` / `clear_child_pid`, `BufReader` over stdout. This duplicates Cursor (and partially Claude) **by design** unless the codebase introduces a shared subprocess helper—acceptable for a new backend but increases maintenance cost when spawn policy changes.
- **Intentional differences:** No `with_progress` / no per-line `ProgressEvent` wiring (see evaluation report). No Cursor-specific resume line-skip or `agent_output` streaming path in Codex backend code.

---

## SOLID / open–closed (`CodingBackend`)

- **`CodingBackend` implementation:** Thin `async fn invoke` via `spawn_blocking` + `name()`—matches Cursor’s pattern; **dependency inversion** preserved (workflow talks to trait / `AnyBackend`).
- **Open–closed:** Adding Codex extended `AnyBackend` and match arms in `mod.rs` without changing the trait—standard and appropriate.
- **Single responsibility:** `CodexBackend` owns process invocation; JSONL shape knowledge lives in `stream/codex.rs`—good separation.

---

## Module boundaries

- **`packages/tddy-core/src/backend/codex.rs`:** CLI process, argv, prompt merge, `CodingBackend` impl.
- **`packages/tddy-core/src/stream/codex.rs`:** Pure parse of stdout lines → `CodexStreamResult`; no I/O—clean.
- **`tddy-coder/src/run.rs`:** Resolution of binary path, `create_backend` arm, `verify_tddy_tools_available` inclusion, CLI fields—appropriate orchestration layer; no Codex protocol logic leaked into `run.rs`.

---

## Documentation

- **Module docs:** `backend/codex.rs` and `stream/codex.rs` both have `//!` summaries describing CLI entrypoints and JSONL semantics—good.
- **Mapping documentation:** `build_codex_exec_argv` documents fresh vs resume argv shape and **explicitly** maps `GoalHints::agent_cli_plan_mode` + `PermissionHint` to `--sandbox` and `--ask-for-approval`—this is the right place; satisfies “inline comments where mapping is documented.”
- **Cross-reference:** `merge_codex_prompt` points to `CursorBackend` for precedence—helps reviewers verify parity.

---

## Test quality

- **`backend/codex.rs` unit tests:** Cover argv shape (fresh, resume, model), merge behavior, and a plan-hint mapping test. The test `codex_exec_argv_maps_plan_goal_hints_to_flags` asserts presence of sandbox/approval-related argv pieces via a somewhat **loose** predicate (`contains("approval")`, etc.); it still passes because `--ask-for-approval` and `--sandbox` are present—could be tightened to exact flag pairs for clearer regression signal.
- **`codex_backend.rs` integration tests:** Mirror `cursor_backend.rs` (shell stub, captured argv, fixture JSONL). Good coverage: exec + `--json`, resume ordering, `-m`, merged system prompt, nonzero exit with `Ok(InvokeResponse)`, missing binary → `BinaryNotFound`. **Duplication:** Each test repeats script write + chmod + tmp layout—same as many Cursor tests; a small shared helper would DRY but is consistent with existing integration style.
- **Platform:** All integration tests gated with `#[cfg(unix)]` (Cursor’s first test is not always gated the same way)—fine for sh stubs on Unix.
- **Gap (product, not test smell):** No test asserts `progress_sink` behavior for Codex; aligns with evaluation note that ProgressSink is not driven from JSONL.

---

## Summary

| Area              | Assessment                                                                 |
|-------------------|----------------------------------------------------------------------------|
| Naming            | Mostly aligned with Cursor/Claude; minor log-prefix inconsistency.        |
| Complexity        | `invoke_sync` is long; acceptable parity with Cursor; refactor optional.   |
| Duplication       | High overlap with Cursor spawn/env/logging; shared helper would be future win. |
| SOLID / OCP       | Trait + `AnyBackend` extension is clean.                                   |
| Modules           | Backend vs stream split is clear.                                        |
| Documentation     | Strong module and argv-mapping docs.                                     |
| Tests             | Solid integration mirror of Cursor; one unit test could be stricter.      |

**Verdict:** The Codex feature matches established backend patterns and keeps parsing in `stream/`. Main clean-code debt is **structural duplication** of the subprocess/conversation-file block with Cursor and the **size of `invoke_sync`**, not naming or module placement.
