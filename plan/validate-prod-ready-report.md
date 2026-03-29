# Validate production-ready: Codex CLI backend

**Scope:** Refactor validation of Codex integration per `plan/evaluation-report.md` and inspected sources only (no invented APIs).

---

## Executive summary

The Codex path is structurally aligned with other CLI backends (spawn, env, `path_with_exe_dir`, child PID tracking, `InvokeResponse`). **High-severity** gaps vs Cursor/Claude: **non-zero process exit does not produce `BackendError`**—callers get `Ok(InvokeResponse)` with `exit_code` set, while Cursor maps non-zero exit to `InvocationFailed` and Claude does the same with a plan-mode exception. **Medium-severity:** `ProgressSink` / `AgentExited` is not emitted from Codex (Cursor and Claude do). **Low-severity:** `log::info` command lines can include the start of the merged prompt (truncated per-arg, not per-command), which may increase sensitive-data surface in logs.

---

## 1. Error handling

| Area | Behavior | Severity | Notes |
|------|----------|----------|--------|
| **Spawn `NotFound`** | `BackendError::BinaryNotFound(path)` | OK | `codex.rs` maps `ErrorKind::NotFound` explicitly. |
| **Other spawn I/O** | `BackendError::InvocationFailed(e.to_string())` | OK | Consistent with stringly errors elsewhere. |
| **stdout capture missing** | `InvocationFailed("failed to capture stdout")` | OK | Defensive after `spawn`. |
| **conversation file open** | `InvocationFailed` with path + source error | OK | Fails fast before streaming. |
| **system_prompt_path read** | `InvocationFailed` with path + source error | OK | Same pattern as merge helper. |
| **`tokio::task::spawn_blocking` join** | `InvocationFailed` | OK | Task panic / cancel surfaced as string. |
| **Non-zero exit code** | Returns **`Ok(InvokeResponse)`** with `exit_code` | **High** | **Inconsistent with `CursorBackend` and `ClaudeCodeBackend`**, which return `Err(BackendError::InvocationFailed(...))` for non-zero exit (Claude allows an exception when structured plan output is present). Workflow `BackendInvokeTask` does not inspect `exit_code` today; behavior depends on submit path and parsers—risk of treating a failed CLI run as a normal step if submit still occurred or errors are ambiguous. |
| **JSONL parse failures** | No `BackendError`; parser fills `result_text` with error summary or logs recoverable cases | **Low** | Surfaces problems in `output` instead of failing the invoke; may or may not match product expectations for “hard fail on bad stream.” |

**Recommendations**

1. **High:** Decide whether Codex should match Cursor semantics (`Err` on non-zero exit) or document Codex as intentionally “soft” and ensure **every** downstream consumer checks `exit_code` (today `packages/tddy-core/src/workflow/task.rs` does not). If matching Cursor, add non-zero handling analogous to `cursor.rs` (lines 367–389), with any Codex-specific escape hatches justified and tested.
2. **Low:** If malformed JSONL should abort the goal, return `BackendError::InvocationFailed` from the backend when parse errors dominate and no usable `session_id`/text exists (parser already detects that case in `stream/codex.rs`).

---

## 2. Logging (levels and noise)

| Logger | Content | Severity | Notes |
|--------|---------|----------|--------|
| `log::info` | `[tddy-codex] command: …` via `format_command_for_log(..., 200)` | **Low** | Full argv includes **merged prompt** as last argument; each arg truncated to 200 chars but **prompt may still appear in info-level logs**. |
| `log::info` | Process exit `code=` + `goal_id` | OK | Useful operational signal. |
| `log::info` | JSONL parse error counts (empty vs recoverable) | OK | Could be noisy on bad streams; still bounded. |
| `log::debug` | Resolved binary, cwd, goal/model/session, per-line stdout byte length, stderr body, merged prompt length, argv length, event types | OK | Appropriate for debug; per-line stdout length logs scale with JSONL line count. |

**Recommendations**

1. **Low:** For production log hygiene, log **only** binary + subcommand flags at `info`, and move full argv (or prompt-bearing tail) to `debug`, or replace prompt in logs with `(prompt, N bytes)`.
2. Keep stderr at `debug` when non-empty (current behavior)—matches desire to avoid noise at default levels.

---

## 3. Configuration (CLI + YAML + env)

| Source | Field / mechanism | Precedence (as implemented) |
|--------|-------------------|-----------------------------|
| **CLI** | `--codex-cli-path` with `env = "TDDY_CODEX_CLI"` | Clap merges env into the same option. |
| **YAML** | `codex_cli_path` in `Config` | Applied in `merge_config_into_args` only when CLI did not set `codex_cli_path` (`config.rs`). |
| **Default** | `CodexBackend::DEFAULT_CLI_BINARY` (`"codex"`) | Used when neither path nor `TDDY_CODEX_CLI` is set (`resolve_codex_binary` in `run.rs`). |

**Precedence:** CLI > config file > `TDDY_CODEX_CLI` (via clap) > `"codex"`. Document this for operators; it mirrors the Cursor resolver pattern.

**Recommendations**

1. **Low:** Ensure user-facing docs state the same order as `resolve_codex_binary` comment (CLI/config vs env interaction is subtle because clap binds env to the flag).

---

## 4. Security

| Topic | Assessment | Severity |
|-------|------------|----------|
| **Secrets in argv/env** | No OpenAI/API keys added by this code; only `PATH`, `TDDY_SOCKET`, `TDDY_REPO_DIR`, `TDDY_SESSION_DIR` plus inherited Codex behavior. | OK |
| **Sandbox** | Plan + read-only → `--sandbox read-only`; else `--sandbox workspace-write`. | OK — explicit mapping in `build_codex_exec_argv`. |
| **Approvals** | Always `--ask-for-approval never` for non-interactive runs. | **Info** — Matches evaluation report; **reduces interactive safety** by design; ensure this is acceptable for all goals (editing goals get workspace-write + no approval prompts). |
| **Logging** | Possible prompt snippet at `info` (see §2). | **Low** |
| **stdin** | `inherit_stdin` when `request.inherit_stdin`; otherwise `Stdio::null()`. | OK |

**Recommendations**

1. **Info:** Revisit whether editing goals should ever use a stricter approval mode when `inherit_stdin` is true (product decision—not a code bug by itself).

---

## 5. Performance

| Topic | Assessment | Severity |
|-------|------------|----------|
| **Async runtime** | Whole invoke runs in `spawn_blocking`; avoids blocking the async executor. | OK |
| **Stdout** | `BufReader::lines()` over piped stdout; lines accumulated in `Vec<String>` until EOF. | **Low** — Large JSONL sessions mean **memory scales with output**; same general pattern as buffering full output elsewhere. |
| **Stderr** | Separate thread `read_to_string` — avoids stderr pipe fill-up. | OK |
| **Conversation file** | `flush()` after each line | **Low** — Syscall-heavy for very chatty streams; acceptable for typical CLI runs. |
| **Progress / streaming** | JSONL is parsed only **after** the process completes; `ProgressSink` not fed during stream. | **Medium** — UX/TUI parity with Cursor stream processing is missing (per evaluation report). |

**Recommendations**

1. **Medium:** If long runs are expected, consider incremental parsing + `ProgressSink` hooks (evaluation report already flags `ProgressSink` not driven from JSONL).
2. **Low:** Batch or buffer conversation-file writes if profiling shows flush overhead.

---

## 6. Consistency vs Cursor backend (exit code semantics)

| Aspect | Cursor (`cursor.rs`) | Codex (`codex.rs`) |
|--------|----------------------|-------------------|
| Non-zero exit | **`return Err(BackendError::InvocationFailed(...))`** before `Ok(InvokeResponse)` | **`Ok(InvokeResponse)`** with `exit_code` set |
| stderr on failure | Included in error message / warn logs | Logged at **debug** only; full buffer returned in `InvokeResponse.stderr` on success path |
| `ProgressSink` / `AgentExited` | Emitted after successful stream processing | **Not emitted** |
| Stream processing | Incremental `process_cursor_stream` with progress | Post-hoc `parse_codex_jsonl_output` on collected lines |

**Recommendations**

1. **High:** Align exit semantics with Cursor **or** explicitly document and test the “soft exit” contract and enforce `exit_code` checks where outcomes matter.
2. **Medium:** Emit `ProgressEvent::AgentExited` for Codex when a `progress_sink` is present, for presenter/TUI parity with Cursor/Claude.
3. **Medium:** Consider non-zero exit → `Err` when `stderr` indicates hard failure even if JSONL partially parsed (product-dependent).

---

## 7. `tddy-coder` wiring (`run.rs`)

- **`verify_tddy_tools_available`:** Codex is **not** exempt (unlike `stub` / `claude-acp`); startup fails early if `tddy-tools` is missing. **OK** for agents that rely on `tddy-tools submit`.
- **`create_backend`:** Selects `AnyBackend::Codex(CodexBackend::with_path(resolve_codex_binary(...)))`. No eager `which` at startup; first invoke fails with `BinaryNotFound` if path wrong—consistent with lazy validation pattern.
- **`resolve_codex_binary`:** Matches documented precedence.

---

## 8. Trace to evaluation report

- Confirms: argv contract, sandbox/approval mapping, binary path/env/YAML, `tddy-tools` requirement.
- Confirms gaps: **ProgressSink not driven from JSONL**; **exit / `InvokeResponse` semantics vs workflow** need explicit product handling.
- Operational: remove stray `.codex-red-test-output.txt` before ship (per evaluation report)—not a code defect but **repo hygiene**.

---

## Severity legend

- **High:** Can cause incorrect success paths or inconsistent failure modes vs other backends.
- **Medium:** UX, observability, or parity gaps with measurable user impact.
- **Low:** Hardening, hygiene, or edge-case improvements.
