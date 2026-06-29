# Changeset: unified tddy-actions → tddy-tasks with optional sandbox execution

**Date:** 2026-06-29  
**Packages:** `tddy-actions` (new), `tddy-task`, `tddy-service`, `tddy-daemon`, `tddy-coder`, `tddy-build`, `tddy-core`, `tddy-tools`, `tddy-sandbox`  
**Feature PRD:** [docs/ft/daemon/1-WIP/PRD-2026-06-29-unified-actions-tasks-sandbox.md](../../ft/daemon/1-WIP/PRD-2026-06-29-unified-actions-tasks-sandbox.md)

## Plan-mode summary

Unify Claude CLI, Bash, tddy-coder, tddy-build BuildActions, session actions (`session_actions`/`session_action_jobs`/`session_action_pipeline`), and fast tools under `tddy-actions` → `tddy-task`. Optional `tddy-sandbox` confinement with inputs RO-mounted and `output_dir` RW. Backend only; new `actions.ActionService` RPC; terminal-sessions RPCs preserved as compat layer.

## TODO

### Planning
- [x] Create/update PRD documentation
- [x] Create changeset

### M1 — tddy-actions spec
- [x] `packages/tddy-actions` workspace member + `ActionSpec` / `SessionActionExtras` / I/O types
- [x] Session manifest + build action conversion helpers
- [x] Unit tests (`packages/tddy-actions/tests/spec_unit.rs`)

### M2 — tddy-task PTY channel
- [x] `ChannelKind::Pty` + `TaskChannel::pty()`
- [x] Proto mapping in `task_service.rs`

### M3 — ActionRuntime
- [x] `ProcessRuntime`, `PipelineRuntime`, `result_kind` post-processors

### M4 — PtyRuntime + PTY unification
- [x] `PtyRegistry` side-table; refactor `ClaudeCliSessionManager`

### M5 — SandboxRuntime
- [x] `SandboxRuntime` wires `SandboxRequest` + registers tasks (`sandbox_runtime.rs`)
- [x] Acceptance tests (`packages/tddy-daemon/tests/action_sandbox_acceptance.rs`)
- [x] **Follow-up done:** confined `spawn_plan` + egress-log I/O bridge (process path) and runner PTY path (`sandbox_action.rs`)
- [x] `working_dir` / `SandboxSpec.cwd` through plan builder and platform spawn
- [x] `extra_read_paths` on `StartAction` params → RO reads in SBPL
- [x] `channel_mode=pty` via `tddy-sandbox-runner --pty-command` + host relay → `TaskChannel` PTY
- [x] `SandboxRequest.stdin` for non-interactive confined CLIs (e.g. tddy-coder plan approval)
- [x] `SandboxRecipe::RunnerPty` policy (Rust runner + in-jail PTY)

### M6 — tddy-coder `--output-dir`
- [x] Flag + artifact rooting (`packages/tddy-coder/tests/output_dir_acceptance.rs`)

### M7 — tddy-build executor delegation
- [x] `run_action` → `ProcessRuntime::spawn` via `action_convert.rs`

### M8 — Session actions unification
- [x] `session_action_jobs` → per-session `TaskRegistry`; `job_id == task_id`
- [x] `session_actions/runtime.rs` + manifest → `ActionSpec`
- [x] Pipeline integration tests pass (`session_action_pipeline_integration.rs`)

### M9 — actions.ActionService RPC
- [x] `actions.proto` + `ActionServiceImpl` + registration

### M10 — Terminal RPC compat layer
- [x] Terminal RPCs over `TaskRegistry` + `PtyRegistry`; acceptance tests pass

### M11 — Integration + validation
- [x] Full `./test` pass (see Validation Results)
- [x] `cargo fmt`, `cargo clippy -- -D warnings` on changed packages

## Acceptance tests

- [x] `list_tasks_includes_a_running_claude_cli_and_bash_task` — `task_service_acceptance.rs`
- [x] `cancel_task_cancels_a_bash_pty_task` — `task_service_acceptance.rs`
- [x] `start_action_bash_returns_task_id_visible_in_list_tasks` — `action_service_acceptance.rs`
- [x] `sandboxed_bash_action_writes_to_output_dir` — `action_sandbox_acceptance.rs`
- [x] `sandboxed_tddy_coder_writes_under_output_dir` — `action_sandbox_acceptance.rs` (macOS)
- [x] `sandboxed_build_action_with_ro_mount` — `action_sandbox_acceptance.rs` (macOS)
- [x] `sandboxed_action_denies_write_outside_egress` — `action_sandbox_acceptance.rs` (macOS)
- [x] `sandboxed_bash_pty_action_streams_output` — `action_sandbox_acceptance.rs` (macOS)
- [x] `sandboxed_action_unsupported_off_darwin_linux` — `action_sandbox_acceptance.rs` (non-macOS/Linux)
- [x] `session_action_manifest_round_trips_through_action_spec` — `spec_unit.rs`
- [x] `pipeline_action_runs_mapper_primary_transform_and_validates_output` — `pipeline_runtime_unit.rs`
- [x] Existing terminal + session-action acceptance tests unchanged

## Sandbox architecture (follow-up direction)

**Principle:** `tddy-sandbox` is a generic jail runtime. Product recipes live in **`tddy-sandbox-recipes`**.
Orchestration (`tddy-daemon` `sandbox_plan_builder`, `sandbox_runtime`) builds plans from actions
and spawns from outside — the sandbox crate never imports `tddy-actions`.

| Layer | Crate / module | Responsibility |
|-------|----------------|----------------|
| Generic jail | `tddy-sandbox` | `SandboxBuilder` → `SandboxPlan`, `exec_reads`, `scratch_runner_env` |
| Recipes | `tddy-sandbox-recipes` | Claude CLI, shell policy, `build_runner_plan` / `build_process_plan` |
| Orchestration | `tddy-daemon` | `sandbox_plan_builder::build_action_sandbox_plan`, `SandboxRuntime` |

**Done (2026-06-29 split):** `claude_spawn.rs` removed from `tddy-sandbox`; Claude/MCP moved to
`tddy-sandbox-recipes::claude_cli`; `build_sandbox_plan` delegates to recipes.

**Done (2026-06-29):** `SandboxRuntime` spawns confined actions via platform `spawn_plan`:
process path tails egress logs into task channels; PTY path uses `build_action_runner_plan` +
`run_host_relay` (same stack as claude-cli sessions).

**Out of scope (unchanged):** `StartAction` RPC params for full `build-action` manifest (harness
test covers jail); config-driven cgroup limits; web UI for sandbox actions.


## New / key files

| Path | Role |
|------|------|
| `packages/tddy-actions/` | `ActionSpec`, runtimes, catalog, conversions |
| `packages/tddy-daemon/src/action_service.rs` | `ActionServiceImpl` |
| `packages/tddy-daemon/src/pty_registry.rs`, `pty_runtime.rs` | PTY unification |
| `packages/tddy-daemon/src/sandbox_action.rs` | Confined process + PTY execution |
| `packages/tddy-daemon/src/sandbox_plan_builder.rs` | Action → `SandboxPlan` / runner plan |
| `packages/tddy-service/proto/actions.proto` | `ListActionKinds`, `StartAction`, `GetAction` |
| `packages/tddy-core/src/session_actions/runtime.rs` | Session actions via `ProcessRuntime` |
