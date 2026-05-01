# Session actions (`tddy-tools`)

## Purpose

Session **actions** are declarative manifests under a canonical session directory. Each manifest describes how to invoke a bounded shell command (`command` argv), optional constraints (CPU **`architecture`**), validated JSON inputs (`input_schema`), optional output-shape hints (`output_schema`), and optional **`result_kind`** processors (for example **`test_summary`** for cargo-style totals). **Automation and agents** consume the same abstraction through **`tddy-tools`** rather than raw ad hoc shells.

Operational trust boundary: manifests live beside **`changeset.yaml`** under **`{sessions_base}/sessions/{session_id}/`** (see [session-layout.md](session-layout.md)); anyone who controls that subtree controls which programs run under the invocation rules described here.

## On-disk layout

- **Directory**: `<session_dir>/actions/`
- **Files**: `<name>.yaml` or `<name>.yml` where `<name>` is the invocation handle passed to **`--action`**.
- **Companion state**: **`changeset.yaml`** supplies optional **`repo_path`**, used as process working directory when the file exists and the field is non-empty. When **`changeset.yaml`** is absent, invocation uses the **`--session-dir`** path as working directory.

## Manifest fields (`ActionManifest`)

| Field | Requirement | Role |
|-------|-------------|------|
| **`version`** | Required | Unsigned integer; manifests use **`1`** in shipped examples. Serde rejects unknown YAML keys (`deny_unknown_fields`). |
| **`id`** | Required | Stable string identifier (mirrors authoring intent; listed in **`list-actions`**). |
| **`summary`** | Required | Human-readable one-liner for listings. |
| **`architecture`** | Required | **`native`** matches the current runtime; an explicit rustc-style triple prefix equal to **`std::env::consts::ARCH`** matches; mismatches yield a descriptive error before spawn. |
| **`command`** | Required | Non-empty argv vector: **`command[0]`** is the program; remaining elements are literal arguments (no templating from JSON yet). |
| **`input_schema`** | Optional | JSON Schema object (**Draft 7** toolchain default) enforced against **`--data`** JSON before any process spawn. Absent schema skips argument validation aside from parsing a JSON object/string where applicable. |
| **`output_schema`** | Optional | Declared shape for tooling; **`list-actions`** reports presence via **`has_output_schema`**. Runtime validation against process output remains a roadmap item. |
| **`result_kind`** | Optional | When set to **`test_summary`**, **`invoke-action`** and blocking **`invoke_session_action`** attach a **`summary`** object with **`passed`**, **`failed`**, **`skipped`** parsed from combined stdout/stderr (cargo **`test result:`** totals line), merged via **`finalize_invocation_record`**. |
| **`output_path_arg`** | Optional | Names a string field in **`--data`**; the value is resolved inside the canonical session directory or **`repo_path`** before spawn. Escape outside those roots terminates with exit code **`3`** and does not execute the command. |

## CLI surface (`tddy-tools`)

### `list-actions`

- **Usage**: **`tddy-tools list-actions --session-dir <SESSION_DIR>`**
- **Stdout**: JSON **`{ "actions": [ { id, summary, has_input_schema, has_output_schema }, ... ] }`**, sorted ascending by **`id`**.

### `invoke-action`

- **Usage**: **`tddy-tools invoke-action --session-dir <SESSION_DIR> --action <ID> --data '<JSON>'`**
- **Stdout**: JSON object including at least **`exit_code`**, **`stdout`**, **`stderr`** strings (UTF-8 with replacement for invalid bytes).
- **`result_kind`**: **`test_summary`** adds **`summary: { passed, failed, skipped }`** when totals appear in captured output.

### Exit semantics

| Code | Meaning |
|------|---------|
| **0** | Tooling succeeded (**including** nonzero child **`exit_code`** in the structured JSON—the child outcome is reported, not surfaced as **`tddy-tools`** failure). |
| **3** | Validation / classification error: malformed **`--data`**, JSON Schema rejection, malformed manifest schema compilation, forbidden path bindings. |
| **1** | Other failures (**I/O**, missing manifest binary, unsupported architecture mismatch after checks, **`changeset.yaml`** unreadable corruption, subprocess spawn failures, missing summary totals when **`result_kind`** demands them). |

## Security & operations notes

1. **`command`** executes with host permissions of the invoking user; manifests are intentionally powerful—treat **`actions/`** as sensitive configuration alongside **`changeset.yaml`**.
2. Path resolution combines canonical session roots with lexical normalization on absolutes to block obvious **`..`** escapes relative to **`--session-dir`** prior to traversal checks.
3. Subprocess capture buffers full stdout/stderr in memory; oversized or infinitely chatty commands load the parent process accordingly—run long jobs behind dedicated wrappers if needed.

## Session action jobs (`session_action_jobs`)

The **`tddy_core::session_action_jobs`** module runs the same **`actions/*.yaml`** manifests as **`invoke-action`**, with an optional **non-blocking** admission path: callers receive a **`job_id`**, filesystem paths for **stdout** and **stderr** logs, and can **`wait`** (with optional timeout) or **`stop`** a running job. Manifest resolution (**`resolve_action_manifest_path`**), **`--data`** validation, **`repo_path`** / **`output_path_arg`** checks, **`ensure_action_architecture`**, and **`result_kind: test_summary`** handling share the synchronous **`session_actions`** implementation.

### On-disk layout

- **Registry root**: `<session_dir>/session_action_jobs/`
- **Per job**: `<session_dir>/session_action_jobs/jobs/<job_id>/` contains **`job.json`**, **`stdout.log`**, **`stderr.log`**.

### Library API (`tddy_core::session_action_jobs`)

| Entry point | Role |
|-------------|------|
| **`invoke_session_action`** | With **`SessionActionInvokeOptions { async_start: false }`**, blocks until the subprocess exits and returns the same JSON record shape as **`invoke-action`** (including **`summary`** when **`result_kind`** is **`test_summary`**). With **`async_start: true`**, creates log files, spawns the manifest command in a new process group on Unix, assigns a version-7 UUID **`job_id`**, and returns **`AsyncStarted`** (**`running`** status plus absolute **`stdout_path`** / **`stderr_path`**). |
| **`wait_session_action_job`** | Polls for terminal state. **`timeout_ms: None`** or **`Some(0)`** waits without an upper bound; a positive bound returns **`TimedOut { still_running }`** when the deadline elapses first. |
| **`stop_session_action_job`** | On Unix, sends **`SIGKILL`** to the child process group, reaps the child, and persists **`cancelled`** state. Returns **`UnknownJob`** when the job directory is missing (**`stable_code`** **`unknown_job`**), **`AlreadyFinished`** when the job is already terminal, and **`Stopped`** / **`AlreadyTerminal`** for live cancellation semantics. |
| **`SessionActionJobRegistry::load`** | Ensures **`session_action_jobs/`** and **`jobs/`** exist under **`--session-dir`**. |

### Platform notes

**`wait`**, **`stop`**, and async spawn bookkeeping use **`libc`** on Unix. Non-Unix builds expose **`JobState`** errors for those entry points.

### Related tests (jobs module)

- **`packages/tddy-core/tests/toolcall_jobs.rs`**
- **`packages/tddy-tools/tests/session_action_jobs_acceptance.rs`**

## Session action pipeline (`session_action_pipeline`)

The **`tddy_core::session_action_pipeline`** module provides library helpers for workflow-style **session actions** that need structured env merge, a canonical **`{"args","env"}`** invocation envelope, optional **input mapper** and **output transform** subprocesses, glob resolution for declared paths, and named **capture channels** (`stdout`, `stderr`, and extensions such as **`logs`**). It complements the declarative **`session_actions`** YAML manifests and **`tddy-tools invoke-action`**: callers embed or orchestrate these primitives when building custom pipelines outside the single-shot manifest invoke path.

### Public surface

| Concern | API |
|--------|-----|
| Default env + invocation overrides | **`merge_session_action_env`** — override map wins on duplicate keys. |
| Canonical JSON without a mapper | **`build_invocation_envelope_direct`** — object with **`args`** (string array) and **`env`** (string-valued map). |
| Glob resolution under a base directory | **`resolve_output_globs_sorted`** — sorted, deduplicated file paths; patterns are relative to **`base`**. |
| Channel manifest | **`build_extended_channel_manifest`** — resolves **`stdout`**, **`stderr`**, and **`logs`** under the session directory; optional path overrides for stdout/stderr. |
| Input mapper | **`run_input_mapper_for_envelope`** — writes structured JSON to the child stdin; child receives **`TDDY_SESSION_CHANNEL_MANIFEST_JSON`** (JSON object of channel id → path). Expects a single JSON object on stdout with **only** **`args`** and **`env`** keys; rejects extra keys. |
| Primary command | **`run_primary_action_with_capture_paths`** — **`Command::new(program).args(args)`** (no shell); **`env_clear`** then **`envs`** from the supplied map; captures stdout/stderr to resolved paths (defaults under **`session_dir/capture/`** when omitted). |
| Output transform | **`run_output_transform_and_validate`** — runs transform argv with null stdin and the same manifest env var; trims stdout and validates parsed JSON with **`jsonschema`** against the caller-supplied **`output_schema`**. |
| Errors | **`SessionActionPipelineError`** — mapper/transform failures, envelope validation, glob paths, schema validation, I/O. |

### Operational notes

- Subprocesses for mapper, transform, and primary run with a cleared environment except for explicitly supplied keys (plus **`TDDY_SESSION_CHANNEL_MANIFEST_JSON`** where applicable). Callers supply any required variables (e.g. **`PATH`**) in the env map when needed.
- Glob patterns passed to **`resolve_output_globs_sorted`** use paths representable as UTF-8 strings when joined with **`base`** for the underlying **`glob`** crate.
- Integration coverage for this module lives in **`packages/tddy-tools/tests/session_action_pipeline_integration.rs`** (Unix file permissions in fixtures); unit coverage in **`packages/tddy-core/tests/session_action_resolve_unit.rs`**.

### Related tests (pipeline module)

- **`packages/tddy-core/tests/session_action_resolve_unit.rs`**
- **`packages/tddy-tools/tests/session_action_pipeline_integration.rs`**

## Related tests

Library coverage: **`packages/tddy-core/tests/session_actions_red.rs`**, **`packages/tddy-core/tests/toolcall_jobs.rs`**. Integration coverage: **`packages/tddy-tools/tests/actions_cli_acceptance.rs`**, **`packages/tddy-tools/tests/session_action_jobs_acceptance.rs`**.

## Related documentation

- [Session directory layout](session-layout.md)
- **Implementation**: [`tddy_core::session_actions`](../../../packages/tddy-core/src/session_actions/mod.rs), [`tddy_core::session_action_jobs`](../../../packages/tddy-core/src/session_action_jobs/mod.rs), [`tddy_core::session_action_pipeline`](../../../packages/tddy-core/src/session_action_pipeline.rs), **[`tddy_core` architecture — Session actions](../../../packages/tddy-core/docs/architecture.md#session-actions-session_actions)**, **[Session action jobs](../../../packages/tddy-core/docs/architecture.md#session-action-jobs-session_action_jobs)**, **[Session action pipeline](../../../packages/tddy-core/docs/architecture.md#session-action-pipeline-session_action_pipeline)**; **`tddy_tools::session_actions_cli`** (binary wiring)
