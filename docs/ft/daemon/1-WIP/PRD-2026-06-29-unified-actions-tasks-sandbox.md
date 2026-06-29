# PRD: Unified tddy-actions → tddy-tasks with optional sandbox execution

**Date**: 2026-06-29
**Status**: In planning
**Scope**: Backend only — no web UI changes
**Affected features**:
- [daemon/background-tasks](../background-tasks.md) — TaskRegistry becomes the single runtime for all tools
- [daemon/terminal-sessions](../terminal-sessions.md) — PTY tools (claude-cli, bash) unify into TaskRegistry
- [daemon/claude-cli-session](../claude-cli-session.md) — claude-cli becomes an action
- [coder/sandbox-builder](../../coder/sandbox-builder.md) — sandbox becomes an execution mode for actions
- [build/tddy-build](../../build/tddy-build.md) — BuildAction execution delegates to the unified runtime

## Summary

Re-arrange and unify how long-running tools and sandboxes work so that **every tool** — Claude CLI, Bash,
tddy-coder, and tddy-build actions — is modelled the same way:

1. **Each tool becomes a `tddy-action`.** An action is a declarative spec: a command (or PTY argv), a set
   of inputs, a set of outputs, an environment, and an optional sandbox request. A new crate
   `packages/tddy-actions` owns the `ActionSpec`, the action catalog, and the runtime that turns a spec
   into a cancellable, observable task.
2. **Running a tool produces a `tddy-task`.** Every action invocation spawns a task in the existing
   `tddy_task::TaskRegistry`. It appears in `TaskService.ListTasks`, is watchable via `WatchTask`, and is
   cancellable via `CancelTask` — including the PTY tools (claude-cli, bash) that today live in a separate
   `ClaudeCliSessionManager` registry.
3. **Each action→task can optionally run inside `tddy-sandbox`.** When an action declares a sandbox
   request, its declared `inputs` are mounted read-only into the jail and its declared `output_dir` is
   writable (and persisted host-side). This reuses the existing `SandboxBuilder` / `SandboxPlan` and the
   macOS Seatbelt / Linux cgroups backends.

Backend only: the existing terminal-sessions and tasks RPCs remain the external contract; new
`actions.ActionService` RPCs are added for starting/inspecting action *types*. No web UI work in this PR.

## Background and motivation

Today, background/long-running work is split across three unrelated mechanisms that do not compose:

- **`tddy_task::TaskRegistry`** (`packages/tddy-task`) — the uniform task abstraction introduced by the
  background-tasks feature. Today only `tool_engine.rs` fast tools (Read/Write/Shell/Grep…) and the VM
  build register here.
- **`ClaudeCliSessionManager`** (`packages/tddy-daemon/src/claude_cli_session.rs`) — a *separate* registry
  of PTY-based tools: the main `claude` CLI (id `"main"`, kind `claude-cli`) and Bash tools (kind `bash`),
  managed via `StartTerminalSession` / `StopTerminalSession`. These are NOT in `TaskRegistry`, so they do
  not appear in `ListTasks`, cannot be cancelled via `CancelTask`, and are not watchable via `WatchTask`.
- **`tddy-build` executor** (`packages/tddy-build/src/executor.rs`) — runs `BuildAction`s via raw
  `tokio::process::Command`, with no observability, no cancellation, and no sandbox confinement. It
  already has a `BuildAction { id, command, inputs: FileSet, outputs: OutputDecl, env, working_dir }`
  shape that is *almost* the same concept as a generic action.

The sandbox path is similarly split: `sandbox_session.rs` knows how to spawn `claude-cli` inside a
`SandboxBuilder` jail, but the wiring is claude-specific (`claude_required_reads`, `claude_policy`,
`claude_required_copies`). Bash, tddy-coder, and build actions have no path into a sandbox.

This fragmentation means: (a) two registries to reason about, (b) PTY tools are second-class citizens of
the tasks UI, (c) sandboxing is not a reusable capability, and (d) tddy-build reinvents "run a command
with inputs and outputs" without the observability/sandbox the rest of the system has.

## Proposed changes

### What changes (State A → State B)

| Area | State A (today) | State B (after) |
|------|-----------------|-----------------|
| Tool model | `tool_engine` fast tools + `ClaudeCliSessionManager` PTY tools + `tddy-build` BuildActions — three models | One `tddy-actions` crate: `ActionSpec` + catalog + runtime. All three map onto it. |
| Task registry | `TaskRegistry` holds only fast tools + VM build | `TaskRegistry` holds **all** tools: fast, PTY (claude-cli/bash), tddy-coder, build actions |
| PTY tools | Separate `ClaudeCliSessionManager` registry; not in `ListTasks` | PTY tools are tasks with a PTY channel; `ClaudeCliSessionManager` becomes a thin wrapper that spawns an action→task |
| Sandboxing | claude-only; wiring in `sandbox_session.rs` | Any action with a `SandboxRequest` runs in `tddy-sandbox`; inputs RO-mounted, `output_dir` RW |
| tddy-coder | Standalone CLI binary only | Also a runnable `tddy-coder` action (goal, feature input) spawnable by the daemon |
| tddy-build execution | `run_action` via raw `tokio::process::Command` | `BuildAction` → `ActionSpec`; execution delegated to the `tddy-actions` runtime (gains observability + cancellation + optional sandbox) |
| RPC | `tasks.TaskService` + `ConnectionService` terminal RPCs | + new `actions.ActionService` (StartAction, ListActionKinds, GetAction). `StartTerminalSession` stays as a compat wrapper that calls `StartAction`. `tasks.TaskService` unchanged. |

### What stays the same

- `tddy_task::TaskRegistry`, `TaskStatus`, `TaskChannel`, `TaskBody`, `TaskHandle` — unchanged API; the
  PTY channel is added as a new `ChannelKind::Pty` variant.
- `tasks.TaskService` proto and `TaskServiceImpl` — unchanged (PTY tasks just appear in `ListTasks`).
- `SandboxBuilder` / `SandboxPlan` / `SandboxSpec` and the macOS Seatbelt / Linux cgroups backends —
  reused as-is; new action recipes declare their reads/copies/policy.
- External terminal-sessions RPC contract (`StartTerminalSession`/`StopTerminalSession`/
  `ListTerminalSessions`/`StreamTerminalOutput`/`SendTerminalInput`) — preserved; re-implemented on top
  of the action runtime.
- `tddy-build` DAG / cache / plugin / manifest / lowering layers — unchanged; only `executor.rs`'s
  single-action execution is delegated.

### New crate: `packages/tddy-actions`

- `ActionSpec` — canonical description: `id, kind, command: Vec<String>, inputs: Vec<ActionInput>,
  outputs: Vec<ActionOutput>, env: BTreeMap<String,String>, working_dir: Option<PathBuf>,
  channel_mode: ChannelMode (Pty | PipedStdoutStderr | CombinedStdoutStderr | None),
  sandbox: Option<SandboxRequest>`.
- `ActionInput { host_path, jail_path, writable: bool }` and `ActionOutput { host_path, kind }` — the
  shared input/output model. `tddy-build`'s `OutputSpec`/`FileSet` map onto these.
- `SandboxRequest { output_dir, extra_reads, network, secrets }` — what an action asks of the sandbox.
- `ActionRuntime` trait — `spawn(spec, registry, session_id) -> TaskHandle`. Implementations:
  `PtyRuntime` (claude-cli/bash via `portable-pty`), `ProcessRuntime` (tddy-coder / build actions via
  `tokio::process`), and `SandboxRuntime` (wraps either with `tddy_sandbox` confinement).
- `ActionCatalog` — registry of known action kinds (`claude-cli`, `bash`, `tddy-coder`, `build-action`)
  and their spec factories / recipes (e.g. the existing `claude_required_reads`/`claude_policy` become
  the `claude-cli` recipe).

### Extraction from `tddy-build`

`tddy-build`'s `OutputSpec { path, kind }`, `srcs_to_inputs`, and `outputs_to_decls` express the same
"declared outputs" concept as `ActionOutput`. To avoid two parallel models:

- The input/output declaration helpers and the `BuildAction → ActionSpec` mapping move into
  `tddy-actions` (or a tiny shared leaf that `tddy-build` can depend on without pulling the heavy
  `tddy-task`/`tddy-sandbox` stack — to be finalized in the dev plan, preserving `tddy-build`'s
  standalone-by-default property as much as possible).
- `tddy-build`'s `executor.rs::run_action` is replaced by `ActionRuntime::spawn(spec)` so build actions
  become observable/cancellable/sandboxable tasks. The DAG wave scheduling, content-addressed cache, and
  plugin lowering remain in `tddy-build`.

### New RPC: `actions.ActionService`

| Method | Shape | Purpose |
|--------|-------|---------|
| `ListActionKinds` | Unary | Enumerate known action kinds and their schemas |
| `StartAction` | Unary | Start an action by kind + params; returns the spawned `task_id` |
| `GetAction` | Unary | Fetch action-kind metadata for a running task |

`StartAction` is the single entry point for "run a tool". `StartTerminalSession` becomes a compat
wrapper that calls `StartAction(kind="bash")` (or `kind="claude-cli"` for the main tool on session
start). All methods carry `session_token` for auth, identical to existing services.

## Impact analysis

### Technical

- **New crate** `packages/tddy-actions` (+ workspace member + `tddy-service` proto `actions.proto`).
- **`tddy-task`** gains `ChannelKind::Pty` and a PTY-capable `TaskChannel` (stdin + broadcast stdout +
  replay). This is the one additive change to the task model.
- **`tddy-daemon`** — `ClaudeCliSessionManager` is refactored to spawn PTY tools as tasks in the shared
  `TaskRegistry`; `sandbox_session.rs` generalised from claude-only to any action with a
  `SandboxRequest`; new `ActionServiceImpl` + `ActionServiceServer` registered; `tool_engine.rs` fast
  tools re-expressed as actions (thin wrapper).
- **`tddy-coder`** — gains an action entry point (a small `run_as_action` shim or a library function the
  daemon calls) so the daemon can spawn it as a task, optionally sandboxed.
- **`tddy-build`** — `executor.rs` delegates single-action execution to `tddy-actions`; gains a
  dependency on the new crate (scope to be finalised in dev plan re: standalone property).
- **`tddy-service`** — new `actions.proto` + generated server/client types.

### User

- The tasks UI (out of scope here) will eventually show claude-cli/bash/tddy-coder/build actions
  uniformly. Backend-only this PR: the data is now available via `TaskService`.
- Sandboxed execution of tddy-coder and build actions becomes possible (previously claude-only).
- Cancellation via `CancelTask` works for claude-cli/bash sessions (previously only via
  `StopTerminalSession`/`SignalSession`).

## High-level requirements

1. Every tool invocation (claude-cli, bash, tddy-coder, build-action, and the existing fast tools) is
   represented as a `tddy_task::Task` in the shared `TaskRegistry` and is visible via
   `TaskService.ListTasks`.
2. PTY tools (claude-cli, bash) are spawned and managed through the action runtime; the existing
   terminal-sessions RPC contract is preserved as a compat layer.
3. Any action may declare a `SandboxRequest`; when present, the runtime confines the action in
   `tddy-sandbox` with declared `inputs` mounted read-only and `output_dir` writable.
4. A new `actions.ActionService` exposes `ListActionKinds` / `StartAction` / `GetAction`; `tasks.TaskService`
   is unchanged and continues to track all running tasks.
5. `tddy-build` BuildActions execute through the unified runtime, gaining cancellation, observability,
   and optional sandboxing, without changing the build DAG/cache/plugin contract.
6. No web UI changes; no breaking changes to existing terminal-sessions or tasks RPC clients.

## Session actions (M8)

Dynamically-authored session actions (`tddy-core` `session_actions`, `session_action_jobs`,
`session_action_pipeline`) unify into the same model:

- **`ActionManifest` → `ActionSpec`** via `action_spec_from_session_manifest` / `SessionActionExtras`
  (`input_schema`, `output_schema`, `result_kind`, `architecture`, `output_path_arg`).
- **Async jobs** (`invoke_session_action` with `async_start=true`) spawn via `ProcessRuntime` into a
  per-session `TaskRegistry` (`session_actions/runtime.rs`). `job_id == task_id` (UUID v7).
- **On-disk compat:** `stdout.log` / `stderr.log` under
  `<session_dir>/session_action_jobs/jobs/<job_id>/` are still written (channel-capture-to-file).
- **`wait` / `stop`** map to task status polling and `CancelTask` on the session registry.
- **Pipeline stages** (input mapper → primary → output transform) are implemented in
  `tddy-actions::PipelineRuntime`; `session_action_pipeline.rs` helpers remain for standalone subprocess
  orchestration used by `tddy-tools` integration tests.
- **Daemon path:** `StartAction(kind="session-action")` and `tddy-tools invoke-action` share the same
  `ActionSpec` shape; standalone mode uses in-process `ProcessRuntime` when no daemon is reachable.

## Success criteria

- `ListTasks` returns claude-cli, bash, tddy-coder, and build-action tasks alongside fast-tool tasks.
- `CancelTask` cancels a running claude-cli/bash PTY task (escalating SIGINT → SIGKILL), equivalent to
  today's `StopTerminalSession`.
- A sandboxed tddy-coder action completes with its declared `output_dir` populated host-side and no
  writes outside the declared inputs/outputs.
- A sandboxed build action runs a `cargo build`-equivalent command confined, with declared src inputs
  RO-mounted and declared outputs written to the RW `output_dir`.
- `tddy-build`'s existing acceptance tests pass with execution delegated to the action runtime.
- All existing terminal-sessions and tasks RPC acceptance tests pass unchanged (compat layer).

## Out of scope (this PR)

- Web UI (`/tasks` page, action picker) — explicitly deferred; backend data is exposed this PR.
- Streaming multi-host forwarding for `WatchTask` on remote daemons (existing limitation preserved).
- `setpgid` full-subtree cancellation (existing known gap; not introduced here).
- Config-driven cgroup limit surfaces for sandboxed actions.
- Migrating the VM-build task to the action runtime (it already uses `TaskRegistry` directly; optional
  follow-up).
