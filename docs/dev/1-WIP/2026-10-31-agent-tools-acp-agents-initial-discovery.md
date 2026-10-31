# Initial Discovery: ACP coding agents launch from their tool definitions (acp-agents)

**Changeset**: [2026-10-31-agent-tools-acp-agents.md](./2026-10-31-agent-tools-acp-agents.md)
**Stack**: `#agent-tools 11/12`
**Date**: 2026-10-31
**Passes**: 1 (Exploration 1 is the whole-stack discovery, copied in full)

## Combined Conclusions

Whole-stack conclusions as planned; this node's slice is named in its changeset's State A.


1. **Three unrelated spawn descriptors exist.** `tddy_actions::ActionSpec` (+ `SandboxRequest`), `tddy_pty::PtySpawnSpec`,
   and the coding-agent path's hand-built `tddy-sandbox-runner` argv + `SandboxRunnerSpawn`. Coding-agent sessions never
   touch `ActionSpec`. `ActionSpec` has serde derives but **no file format** — nothing loads it from YAML.
2. **`ActionSpec` sandboxing is thin and partly fake.** `SandboxRequest{output_dir, extra_read_paths, recipe, stdin}`;
   only the (client-less) `actions.ActionService.StartAction` RPC attaches it. `ProcessRuntime` ignores `sandbox`,
   `inputs`, `outputs`; `ChannelMode::Pty` allocates no PTY. Sandboxed PTY mode hard-codes `RunnerPty`, ignoring the
   recipe. `PipelineRuntime` has no production consumer. Several docs over-claim (PtyRuntime/ActionKind in tddy-actions,
   inline spec, session manifests in ListActionKinds).
3. **tddy-build's `tool` target is the Bazel-ish anchor.** `{bin_dir, commands}` — only `bin_dir` used (PATH for actions
   naming it in `tool_dep_ids`); `ActionType::TOOL` declared, not implemented. No sandboxing ("hermetic sandboxing" listed
   as not implemented). Session-action manifests (`actions/<id>.yaml`) are the only runtime YAML action specs, always
   unconfined.
4. **The sandbox plan vocabulary is already declarative-shaped but Rust-only.** `tddy_sandbox::SandboxPlan`
   (reads/mounts/copies/symlinks/env+secrets/policy/network/limits/cgroup/stdin) — no serde. Agent knowledge is encoded as
   `SandboxRecipe` enum arms (ClaudeCli, CursorCli, OpenCodeCli, MuseCli, …) in `tddy-sandbox-recipes`: exec reads,
   `PolicySpec`, credential copies, `CLAUDE_CODE_OAUTH_TOKEN` secret, `allow_oauth_inbound`, env overlays. Unknown recipe
   names silently become `Generic`. The recipe is **inferred by sniffing runner argv** in three layers.
5. **The in-jail runner is agent-aware.** `SandboxRunnerArgs` carries `--claude-binary`, `--agent-kind
   claude|cursor|opencode|muse`, `--claude-arg`, `--permission-mode`, `--resume`, `--append-system-prompt-file`…; the
   runner builds agent argv itself and writes MCP config (Claude `$TMPDIR/claude-mcp-config.json`, Cursor
   `$HOME/.cursor/mcp.json`). Only the generic `--pty-command` mode is tool-agnostic, and it **lacks the egress-proxy env
   and secret injection** the agent paths get.
6. **Agents are told they are sandboxed** via `TDDY_SANDBOX_*`, `TDDY_EGRESS_SHIM`, proxy env, `TDDY_SECRET_*`,
   `MANAGED_CODEBASE_PREAMBLE` in `CLAUDE.md`/`AGENTS.md`, and per-agent tool flags. In-jail `tddy-tools` switches transport
   on `TDDY_SANDBOX_TOOL_IPC` — tddy-tools is sandbox-aware by design (it is the relay), the agent need not be.
7. **Agent enumeration is duplicated ~10×**: `AnyBackend`, `Harness`, `HeadlessHarness`, `CLI_HARNESSES`, `SandboxRecipe`,
   sandbox-app `AgentKind`, runner `--agent-kind`, `ACP_HARNESSES`/`HEADLESS_HARNESSES`, `BUILTIN_BACKEND_AGENT_IDS`, two
   backend-construction matches, plus string gates on `session_type == "claude-cli"` across lifecycle/activity/rpc. Per-agent
   sandboxed start functions in `tddy-agent-launch` (`svc_start_sandboxed_{claude,cursor,cli_harness}_session`, relaunch) are
   near-copies (76% overlap, todo 2026-09-24). Selection: `session_type`, `--agent`, daemon.yaml `claude_cli:`/`cursor_cli:`/
   `cli_harnesses.*`, `<tddyhome>/agents/*.yaml` harness profiles, ~15 binary env overrides.
8. **Declarative precursors already exist**: `CliHarness` descriptors (fn-pointer argv), `HarnessRoute` (pure data),
   `HarnessProfile` YAML, `SandboxAppConfig` (agent-centric YAML), and the **designed-but-unbuilt `ToolSpec[]` /
   `<tddyhome>/tools/*.yaml`** (todo `2026-09-26-grep-is-unreachable-inside-every-jail.md`: a tool names a host binary or a
   tddy-build target and states what becomes readable, executable and reachable on PATH; unsatisfiable → refuse at jail start).
9. **What cannot be YAML (runtime protocol, stays Rust behind a named adapter)**: stream parsers (claude stream-json,
   cursor, codex JSONL, oz NDJSON), ACP driver + per-agent `AcpAgentHooks`, Claude/Cursor hook protocols, Codex OAuth
   capture/relay, Warp resume-token scraping, model-list parsing. A YAML tool can only *name* one of these.
10. **Launch-time knowledge that can be YAML**: binary resolution (env override, default, search paths), argv templates
    (model / resume / session-id / permission / prompt slots), env, home strategy (persistent vs per-session, seed-once
    files, install mirrors), tmp overlays, credential copies + secrets, MCP registration placement (file path + format, or
    argv/inline), tool allow/deny flags, context globs + preamble files, exec reads / policy, egress (shim on/off; no host
    allowlist exists yet), OAuth inbound, PTY vs stdio vs ACP transport.
11. **No code issue in the affected crates is claimed** by another PR (agent-backend 8, agent-launch 8, coding-subagent 2,
    sandbox 1, sandbox-runner 3 — all unclaimed). tddy-actions, tddy-sandbox-recipes, tddy-build have no `code-issues/`
    directory (not analyzed ≠ clean).


## Exploration 1 — whole-stack discovery (three parallel passes, 2026-10-31)

### Pass 1 — declarative tool/action machinery (tddy-actions, tddy-build, tddy-tools, tddy-task)

#### Discovery dump: existing declarative tool/action machinery

Repo: `/Users/mantasi/Code/tddy-coder/.worktrees/redesign-coding-agents`. Read-only pass, very thorough.

---

#### 1. Sequence (chronological)

1. `ls` on tddy-actions, tddy-build, tddy-task, tddy-tools, tddy-tools-cli, plus `wc -l` on the src files. Purpose: get the layout and sizes.
2. `git ls-files | grep -iE '(\.action\.ya?ml|/actions/|/tools/.*\.ya?ml|catalog|\.tool\.ya?ml|BUILD\.ya?ml)'` and every tracked `*.yaml`. Purpose: find any spec files on disk.
3. `cat` of tddy-actions `Cargo.toml`, `lib.rs`, `spec.rs`, `catalog.rs`, `error.rs`, `result_kind.rs`. Purpose: the core schema.
4. `cat` of tddy-actions `convert.rs`, `pipeline.rs`, `process_runtime.rs`. Purpose: the runtimes and converters.
5. `ls -R docs` and `cat` of the tddy-actions changesets; `wc` on its tests. Purpose: history and claims.
6. `cat` of tddy-actions `tests/*.rs`. grep for `tddy-actions` in every Cargo.toml and `tddy_actions` in `*.rs`. Purpose: find consumers. (The first `--include=*.rs` failed under a zsh glob, so I re-ran it with quotes.)
7. grep for `PtyRuntime|SandboxRequest|spec.sandbox|extra_read_paths|…` in packages, and `grep -l` for "ActionSpec|action spec|tddy-actions" in docs. Purpose: find where sandboxing is consumed and which docs mention it.
8. `cat` of tddy-daemon-sandbox `sandbox_runtime.rs` and `sandbox_action.rs`. Purpose: what the runtime does with `SandboxRequest`.
9. `cat` of tddy-daemon-sandbox `sandbox_plan_builder.rs` and `action_service.rs`.
10. grep the tddy-sandbox-recipes API, and `cat` of `tddy-service/proto/actions.proto`.
11. Read `tddy-sandbox-recipes/src/plan.rs` in full. Purpose: the recipe → policy/reads/copies mapping.
12. `cat` of tddy-build `Cargo.toml`, `lib.rs`, `manifest.rs`, `builtin.rs`, `action_convert.rs`.
13. `cat` of tddy-build `capabilities.rs`, `plugin.rs`, `discovery.rs`, `lower.rs`.
14. `cat` of tddy-build `executor.rs`, and grep for `bin_dir|PATH`.
15. `ls` of tddy-build `proto/` and `cat` of `actions.proto` and `cache.proto`; grep for `tool_dep_ids`.
16. `git grep "type: tool"` in yaml/md, then `cat` of the example pipeline BUILD.yaml files, the tddy-tools and tddy-acp-stub BUILD.yaml files, and the root BUILD.yaml.
17. `cat` of tddy-build `docs/architecture.md`, its 2026-06-29 changeset and `service.rs`.
18. `ls` of tddy-tools and tddy-tools-cli; grep their Cargo deps.
19. Line counts of the tddy-tools CLI files.
20. `cat` of tddy-tools `action_tools.rs`, `session_actions_cli.rs`, and `cli.rs` lines 1-120.
21. grep for `pub struct ActionManifest`; `ls` of session_actions; grep of the tddy-tools-cli `main.rs` dispatch.
22. Read tddy-tools-cli `main.rs` lines 20-115 and tddy-session-store `session_actions/manifest.rs`.
23. `cat` of tddy-session-store `session_actions/runtime.rs`, plus `paths.rs` and `arch.rs` heads.
24. Read tddy-daemon `runtime/mount_entries.rs` lines 225-265; grep for `ActionServiceImpl`/`TaskServiceImpl`; `ls` of tddy-sandbox-app.
25. Read tddy-task `Cargo.toml`, `lib.rs` and `task.rs` lines 1-200.
26. Listed the APIs in tddy-task `task.rs`/`registry.rs`; grep for the session-lifecycle re-export.
27. Read `task.rs` lines 270-345, `registry.rs` lines 95-240, and the tddy-task PTY changeset.
28. grep for the `tddy_session_lifecycle` alias; tddy-daemon-sandbox `lib.rs` header.
29. `cat` of tddy-daemon-sandbox `docs/task-and-action-services.md`; grep of `docs/ft/daemon/background-tasks.md`.
30. `cat` of the 2026-06-29 changesets: ft/daemon changelog, dev changeset, tddy-sandbox, tddy-daemon.
31. Read `background-tasks.md` lines 1-55 and 215-245.
32. Read tddy-pty `runtime.rs` (header, `PtySpawnSpec`, `PtyRuntime::spawn`).
33. grep for the sandbox spec structs in tddy-sandbox, then read `spec.rs` and `builder.rs` lines 35-320.
34. `ls` of tddy-sandbox-runner and grep of the runner flags; read `runner.rs` lines 262-365.
35. `ls` of tddy-tool-engine and its `lib.rs`/`catalog.rs` exports.
36. grep of `docs/ft/build/tddy-build.md`, `docs/ft/coder/session-actions.md` and `docs/ft/coder/sandbox-builder.md`.
37. Read `session-actions.md` lines 1-69.
38. grep for `PipelineRuntime|pipeline-action|PipelineSpec` consumers; read the head of `action_sandbox_acceptance.rs`.
39. `git grep` for ActionService clients (`StartAction`, `ListActionKinds`, `ActionServiceClient`) and for users of `build_runner_plan` / sandbox-recipes.
40. grep of tddy-web/TS for ActionService; read tddy-daemon-sandbox `sandbox_session.rs` lines 760-830 (`build_sandbox_plan`).
41. grep of tddy-bsp `build_cli.rs` relay paths, tddy-tools `cli.rs` relay paths and `pty_relay.rs` sandbox flags; read `cli.rs` lines 380-439 and the head of tddy-sandbox-app `host_actions.rs`.
42. grep of `config.example.yaml` and `sandbox-config.example.yaml`.
43. `git log` on tddy-actions and the action service.

---

#### 2. Inspected files and excerpts

#### tddy-actions

**`packages/tddy-actions/Cargo.toml`**
- Description: "Unified action spec and runtime — tools become tddy-tasks with optional sandbox execution".
- Deps: serde, serde_json, jsonschema, tokio, `tddy-task`.
- There is no serde_yaml dep, so the crate itself has no YAML loading.

**`packages/tddy-actions/src/lib.rs`**
- Doc: "Every tool (Claude CLI, Bash, tddy-coder, build actions, session actions) is described by an [`ActionSpec`] and executed via [`ActionRuntime`], producing a [`tddy_task::TaskHandle`]".
- No `ActionRuntime` trait exists.
- Exports: `ActionCatalog`, the convert fns, `ActionError`, `PipelineRuntime`, `ProcessRuntime`, `apply_result_kind`, and the spec types.

**`packages/tddy-actions/src/spec.rs`** (verbatim key types)
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelMode {
    #[default] None, StdoutStderr, Combined,
    /// PTY byte stream (stdin + broadcast output).
    Pty,
}
pub struct ActionInput {
    pub host_path: PathBuf,
    /// Path inside the jail; defaults to basename of host_path when empty.
    #[serde(default)] pub jail_path: Option<PathBuf>,
    /// When true the path is writable inside the jail (typically only output_dir).
    #[serde(default)] pub writable: bool,
}
pub struct ActionOutput { pub host_path: PathBuf, pub kind: OutputKind }
pub enum OutputKind { #[default] File, Directory }

/// Optional sandbox confinement request.
pub struct SandboxRequest {
    /// Writable egress directory inside the jail (persisted host-side).
    pub output_dir: PathBuf,
    /// Additional read-only paths beyond action inputs.
    #[serde(default)] pub extra_read_paths: Vec<PathBuf>,
    /// Recipe name (`claude-cli`, `bash`, `tddy-coder`, `build-action`, …).
    #[serde(default)] pub recipe: Option<String>,
    /// Bytes written to the confined process stdin after spawn (e.g. plan approval `a\n`).
    #[serde(default)] pub stdin: Option<String>,
}
pub struct SessionActionExtras {
    pub summary: String, pub architecture: String,
    #[serde(default)] pub input_schema: Option<Value>,
    #[serde(default)] pub output_schema: Option<Value>,
    #[serde(default)] pub result_kind: Option<String>,
    #[serde(default)] pub output_path_arg: Option<String>,
    #[serde(default)] pub manifest_version: u32,
}
pub struct PipelineStage { pub program: String, pub args: Vec<String>, #[serde(default)] pub env: BTreeMap<String,String> }

/// Canonical description of a runnable tool.
pub struct ActionSpec {
    pub id: String,
    pub kind: String,
    pub command: Vec<String>,
    #[serde(default)] pub inputs: Vec<ActionInput>,
    #[serde(default)] pub outputs: Vec<ActionOutput>,
    #[serde(default)] pub env: BTreeMap<String, String>,
    #[serde(default)] pub working_dir: Option<PathBuf>,
    #[serde(default)] pub channel_mode: ChannelMode,
    #[serde(default)] pub sandbox: Option<SandboxRequest>,
    #[serde(default)] pub session: Option<SessionActionExtras>,
    #[serde(default)] pub pipeline: Option<PipelineSpec>,
}
pub struct PipelineSpec {
    #[serde(default)] pub input_mapper: Option<PipelineStage>,
    pub primary: PipelineStage,
    #[serde(default)] pub output_transform: Option<PipelineStage>,
    #[serde(default)] pub capture_channel_ids: Vec<String>,
}
```
- `validate()` checks: non-empty id and kind; command non-empty unless there is a pipeline; every `inputs[].host_path` absolute.
- Serde derive: no `rename_all` and no `deny_unknown_fields`. Enum variants therefore serialize as PascalCase (`"StdoutStderr"`, `"Pty"`).
- Nothing in the crate deserializes ActionSpec from a file. It is built only in Rust.

**`packages/tddy-actions/src/catalog.rs`**
- `ActionCatalog` is a unit struct.
- `list_kinds()` returns a hard-coded `["claude-cli","bash","tddy-coder","build-action","session-action","pipeline-action","execute_tool"]`.
- `kind_for_spec` returns `spec.kind`.
- There is no discovery, no files and no per-kind metadata.

**`packages/tddy-actions/src/convert.rs`**
- `SessionManifestFields` → `action_spec_from_session_manifest`:
  - `kind:"session-action"`, `channel_mode: StdoutStderr`, `sandbox: None`.
  - `session: Some(SessionActionExtras{…})`.
- `BuildActionFields { id, command, env, input_globs: Vec<(String,Vec<String>)>, outputs: Vec<(String,OutputKind)>, working_dir }` → `build_action_fields_to_spec(repo_root, …)`:
  - `kind:"build-action"`.
  - Glob patterns become `ActionInput.host_path` as literal `repo_root/root/pattern`, so a glob string is stored as a path.
  - `sandbox: None`.

**`packages/tddy-actions/src/process_runtime.rs`**
- `ProcessRuntime::spawn(registry, spec, session_id)` runs `validate`, builds channels from `channel_mode`, then calls `registry.spawn(body, kind, session_id, channels)`.
- The body:
  - runs `tokio::process::Command::new(command[0])` with args, env added on top of the inherited env, cwd, and piped stdin/stdout/stderr;
  - calls `register_child_pid`;
  - on cancel, does `select!` then kill;
  - reads stdout to the end and only then stderr. It writes whole buffers to the channels after exit, so nothing streams live.
  - finally `apply_result_kind` → `ctx.set_result`.
- **It ignores `spec.sandbox`, `inputs`, `outputs` and `pipeline`.**
- On `ChannelMode::Pty` it creates `TaskChannel::pty("0","pty")` but drops `_stdin_rx`. No real PTY is allocated: the process still uses pipes and writes to channel "0".

**`packages/tddy-actions/src/pipeline.rs`**
- `PipelineRuntime::spawn` requires `spec.pipeline`.
- Steps:
  - the mapper stage's stdout is a JSON envelope `{args, env}`;
  - primary is run as `args[0]` + rest, with `env_clear()` + `spec.env`;
  - the output transform gets the primary's stdout on stdin;
  - the result is validated against `session.output_schema` with jsonschema;
  - `set_result({exit_code, stdout, stderr})`.
- The `pipeline.primary` field is effectively replaced by the envelope args.

**`packages/tddy-actions/src/result_kind.rs`**
- `apply_result_kind(kind, stdout, stderr, exit)` returns `{exit_code, stdout, stderr}`.
- `"test_summary"` adds `summary{passed, failed, skipped}` parsed from the cargo `test result:` line.

**`packages/tddy-actions/src/error.rs`**
- `ActionError::{InvalidSpec, UnknownKind, Spawn, Validation, Io}`.

**`packages/tddy-actions/tests/spec_unit.rs`**
- Covers the session-manifest round trip, the build-fields → spec conversion, and rejection of an empty command.

**`packages/tddy-actions/tests/pipeline_runtime_unit.rs`**
- Mapper/primary/transform plus schema validation using `/bin/sh` stages.

**`packages/tddy-actions/docs/changesets/2026-06-29-unified-action-execution-runtime.md`**
- Claims "`ActionSpec`/`ActionKind`, `ProcessRuntime`, `PtyRuntime` (portable-pty), `PipelineRuntime`…".
- Neither `ActionKind` nor `PtyRuntime` exists in this crate. `PtyRuntime` lives in `tddy-pty` and takes a `PtySpawnSpec`, not an `ActionSpec`.
- Git history: a single feature commit, `bf69809a6 feat: unified actions → tasks with optional sandbox execution (#244)`.

#### Sandbox execution of actions (tddy-daemon-sandbox)

**`packages/tddy-daemon-sandbox/src/sandbox_runtime.rs`**
- `attach_sandbox_request(spec, output_dir, recipe, extra_read_paths, stdin)` canonicalizes the paths and sets `spec.sandbox = Some(SandboxRequest{..})`.
- `SandboxRuntime::spawn` delegates to `spawn_sandbox_action`.

**`packages/tddy-daemon-sandbox/src/sandbox_action.rs`**
- `spawn_sandbox_action` requires `spec.sandbox` and builds channels (PTY mode keeps `stdin_rx`).
- `SandboxActionBody::run`:
  - `layout = ActionSandboxLayout::under_output_dir(output_dir, spec.id)`.
  - **Pty mode** → `run_pty_confined_action`:
    1. `build_action_runner_plan`
    2. `spawn_confined_plan`
    3. `wait_for_sandbox_ready(…120s…)`
    4. `connect_sandbox_session_client(ready_marker, grpc_socket)`
    5. `run_host_relay(client, NullToolHandler, HostRelayConfig::new(session_id, term_tx), stdin_rx)`; terminal bytes go to PTY channel "0"
    6. wait for exit; `set_result` with empty stdout/stderr.
  - **Other modes**:
    1. `build_action_sandbox_plan`
    2. run `run_confined_plan` in `spawn_blocking`, which spawns and polls every 50ms, tailing `egress_log_path(egress, SANDBOX_EXEC_STDOUT_LOG / STDERR_LOG)` into the channels;
    3. on exit, `apply_result_kind`.
- `spawn_confined_plan`:
  - macOS: `tddy_sandbox_darwin::spawn_plan`, or `tddy_sandbox_qemu` if `qemu_backend_requested()`;
  - Linux: `tddy_sandbox_cgroups::spawn_plan` or qemu;
  - other platforms: `SandboxError::Unsupported`.

**`packages/tddy-daemon-sandbox/src/sandbox_plan_builder.rs`**
- `ActionSandboxLayout::under_output_dir`:
  - `project_root = output_dir/sandbox/<action_id>`
  - `scratch = project_root/.work`
  - `egress = output_dir`
  - `profile = project_root/sandbox.sb`
- `input_mounts(inputs)` maps each input to `MountSpec{host, jail: jail_path or host_path, writable}`.
- `resolve_action_cwd`: the working dir must sit under project_root, scratch, egress or a mount.
- `build_action_sandbox_plan(spec, layout, session_id, extra_env)` (process mode):
  - `recipe = recipe_from_name(sandbox.recipe or spec.kind)`;
  - env = `scratch_runner_env(scratch_home, scratch_tmp, session_id, ipc_stub, output_dir)`, plus `claude_runner_env_overlay` for ClaudeCli, plus `extra_env` (= `spec.env`);
  - then `build_process_plan(ProcessPlanRequest{ command: spec.command, mounts, recipe, host_home: $HOME, cwd, extra_reads: extra_read_paths→ReadSpec::subpath(BinaryDeps), stdin })`.
- `build_action_runner_plan` (PTY mode):
  - `recipe = SandboxRecipe::RunnerPty` is hard-coded;
  - it builds the runner argv `[runner, --session-id, --context-dir, --cwd, --grpc-socket, --tool-ipc-socket, --ready-marker, --grpc-listen-port, --egress-shim-port, --model "", --pty-command=<each argv token>]`, plus `--grpc-uds` on Linux;
  - then `build_runner_plan(RunnerPlanRequest{ loopback_allow_ports:[grpc,shim], ipc_socket, mounts, recipe:Some(RunnerPty), host_home, route: None })`.
  - Comment on the route: "The jail runs an action's command and hosts no agent, so it takes no provider route."

**`packages/tddy-daemon-sandbox/src/action_service.rs`** (implements the `actions.ActionService` RPC)
- `kind_metadata` is a hard-coded summary table. For example, `"claude-cli" => "Interactive Claude CLI session (PTY)"` is listed, but StartAction cannot start it.
- `spec_from_start_request(kind, params_json, sandbox)` supports only:
  - `bash` (`params.command` as a string → `sh -c`, or as an array; `id`, `working_dir`, `channel_mode` ∈ pty/stdout_stderr/none/default Combined);
  - `tddy-coder` (`goal`, `coder_binary`, `agent`, `recipe`, `output_dir`, `feature` → argv);
  - `session-action`.
  - Any other kind gets "StartAction does not support kind '{other}' yet".
- If `sandbox`: requires `params.output_dir`; reads `sandbox_recipe`, `extra_read_paths` and `stdin`, then calls `attach_sandbox_request`.
- `start_action`: `SandboxRuntime::spawn` when `req.sandbox`, otherwise `ProcessRuntime::spawn`.
- Auth goes through `SessionUserResolver`.

**`packages/tddy-service/proto/actions.proto`**
- `ListActionKinds`, `StartAction(session_token, session_id, kind, params_json, bool sandbox)`, `GetAction` → `ActionTaskInfo{task_id, kind, session_id, status, exit_code, error_message, created_unix_ms, result_json}`.
- TS bindings are generated (`packages/tddy-web/src/gen/actions_pb.ts`).
- I found no TS or Rust client calling it outside tests.

**`packages/tddy-daemon/src/runtime/mount_entries.rs:238-265`**
- `mount_action_entries` creates `tddy_session_lifecycle::action_service::ActionServiceImpl::new(task_registry, tddy_actions::ActionCatalog::new(), user_resolver)` and registers it as `"actions.ActionService"`.
- `tddy-session-lifecycle/src/lib.rs:31` contains `pub use tddy_daemon_sandbox::*;`.

**`packages/tddy-daemon-sandbox/tests/action_sandbox_acceptance.rs`**
- An RPC-bridge harness (`ActionServiceServer` + `TaskServiceServer` over a shared `TaskRegistry`).
- The doc lists what it covers: output-dir writes, refusal without an output_dir, tddy-coder, a build action under an RO mount, escape denial, PTY streaming, and an unsupported platform.
- The doc notes that `sandboxed_bash_pty_action_streams_output` hangs on macOS.

#### Sandbox vocabulary that a YAML would map onto

**`packages/tddy-sandbox-recipes/src/plan.rs`**
- `SandboxRecipe::{Generic, ClaudeCli, CursorCli, Shell, RunnerPty, OpenCodeCli, MuseCli}`.
- `parse_recipe_name` takes `claude-cli`, `cursor-cli`, `opencode-cli`, `muse-cli`, `bash`|`shell`, `generic`. **Anything else is silently `Generic`**, including `tddy-coder` and `build-action`.
- `detect_recipe_from_argv` maps runner flags to a recipe: `--agent-kind cursor|opencode|muse`, `--claude-binary`, `--pty-command`.
- Per-recipe behavior is hard-coded in Rust match arms:
  - `collect_reads`: e.g. `process_claude_exec_reads`, `process_opencode_exec_reads`, `binary_exec_reads`;
  - `policy_for_recipe` returns a `PolicySpec{allow_dynamic_code_generation, allow_process_fork, mach_lookup, sysctl_read, pseudo_tty, exec_paths}`;
  - `copies_for_recipe` copies credentials for Claude and Cursor;
  - `finish_builder` injects the `CLAUDE_CODE_OAUTH_TOKEN` secret for ClaudeCli when the session is not routed;
  - `NetworkSpec.allow_oauth_inbound` is set for Claude and Cursor when not routed;
  - a route (`HarnessRoute`) supplies env + `route_secrets` + `materialise_route_files` + `route_config_env(recipe)`.
- Request structs: `RunnerPlanRequest{project_root, scratch_dir, egress_dir, profile_path, runner_argv, env, loopback_allow_ports, ipc_socket, mounts, recipe, host_home, route}` and `ProcessPlanRequest{…, command, env, mounts, recipe, host_home, cwd, extra_reads, stdin}`.

**`packages/tddy-sandbox/src/spec.rs`**
- `SandboxSpec{project_root, scratch_dir, egress_dir, allow_read_paths, command, env, profile_path, loopback_allow_ports, ipc_socket, cwd}`.

**`packages/tddy-sandbox/src/builder.rs`** (the full plan vocabulary)
```rust
pub struct SandboxPlan { pub spec: SandboxSpec, pub reads: Vec<ReadSpec>, pub mounts: Vec<MountSpec>,
  pub copies: Vec<CopySpec>, pub symlinks: Vec<SymlinkSpec>, pub env: EnvSpec, pub policy: PolicySpec,
  pub network: NetworkSpec, pub limits: ResourceLimits, pub stdin: Option<Vec<u8>>, pub cgroup: CgroupConfig }
pub struct ReadSpec { pub host: PathBuf, pub jail: Option<PathBuf>, pub kind: ReadKind /*Subpath|Literal|Metadata|Regex*/, pub exec: bool, pub reason: ReadReason }
pub struct CopySpec { pub src: PathBuf, pub dest: PathBuf, pub optional: bool, pub mode: Option<u32> }
pub struct MountSpec { pub host: PathBuf, pub jail: Option<PathBuf>, pub writable: bool }
pub struct SecretSpec { pub env_name: String, pub source: SecretSource /*Value|HostFile*/ }
pub struct EnvSpec { pub vars: BTreeMap<String,String>, pub secrets: Vec<SecretSpec> }
pub struct PolicySpec { allow_dynamic_code_generation, allow_process_fork, mach_lookup: MachPolicy/*All|Names*/, sysctl_read, pseudo_tty, exec_paths }
pub struct NetworkSpec { pub loopback_allow_ports: Vec<u16>, pub allow_oauth_inbound: bool }
pub struct ResourceLimits { memory_max: Option<u64>, cpu_max: Option<String>, pids_max: Option<u64> }
```
- None of these types derive Deserialize in the excerpts I read (`#[derive(Debug, Clone, …)]` only).

**`packages/tddy-sandbox-runner/src/runner.rs:266-362` (`SandboxRunnerArgs`)**
- Flags:
  - `--session-id`, `--context-dir`, `--cwd`, `--grpc-socket`, `--tool-ipc-socket`, `--tddy-tools-path`
  - `--claude-binary` (default "claude")
  - `--agent-kind` (claude|cursor|opencode|muse), `--agent-binary`
  - `--model`, `--ready-marker`, `--permission-mode` (default auto), `--dangerously-skip-permissions`, `--resume`, `--append-system-prompt-file`
  - `--grpc-listen-port`, `--egress-shim-port`, `--grpc-uds`
  - `--pty-command` (repeated: generic confined action mode)
  - `--claude-arg`, `--agent-arg`, `--mcp-log-level`
  - `--stdio` (SandboxService over stdin/stdout)
  - `--initial-cols`, `--initial-rows`
- Agent kinds are an enum baked into the runner. ACP is not among them.

**`packages/tddy-daemon-sandbox/src/sandbox_session.rs:760-830`**
- `SandboxRunnerSpawn{…, mounts, host_home, cgroup, route}` → `build_sandbox_plan` → `build_runner_plan(recipe: None …)`, so the recipe is inferred from the runner argv.
- On macOS it merges `build_allow_read_paths(runner_argv)`.
- This is the coding-agent session path, and it **does not go through ActionSpec**.

#### tddy-pty

**`packages/tddy-pty/src/runtime.rs`**
- `PtySpawnSpec { argv, worktree_path, session_id, terminal_id, kind, env: Vec<(String,String)> }`.
- `PtyRuntime::spawn(registry, pty_registry, spec, ready_tx)` uses `portable_pty`, `TaskChannel::pty("0","pty")`, and `PtyRegistry` for resize/control.
- Doc: "Host-specific concerns (OS-user impersonation, privilege drops, per-user PATH/HOME) are the caller's responsibility". Default size is 24x220.
- This is the real unconfined PTY runtime, and it is not wired to `ActionSpec`.

#### tddy-build

**`packages/tddy-build/Cargo.toml`**
- The description says "Standalone — no tddy-* deps", but it depends on `tddy-actions` and `tddy-task`, so that is stale.
- The same stale claim appears in `docs/architecture.md`.

**`packages/tddy-build/src/manifest.rs`**
```rust
#[serde(default, deny_unknown_fields)] pub struct BuildManifest { pub schema_version: u32, pub targets: Vec<BuildTarget> }
#[serde(default, deny_unknown_fields)] pub struct BuildTarget { pub id: String, pub name: String, pub deps: Vec<String>,
  pub tags: Vec<String>, pub languages: Vec<String>, pub capabilities: Option<TargetCapabilities>,
  pub actions: Vec<BuildAction>, pub config: Option<TargetConfig> }
pub struct TargetCapabilities { pub compile: bool, pub test: bool, pub run: bool, pub debug: bool }
pub struct TargetConfig { pub r#type: String, #[serde(flatten)] pub fields: serde_yaml::Value }
```

**`packages/tddy-build/src/builtin.rs`**
- Built-in types are `SCRIPT="script"`, `TOOL="tool"`, `GROUP="group"`.
```rust
#[serde(default, deny_unknown_fields)]
struct ToolTarget { bin_dir: String, #[allow(dead_code)] commands: HashMap<String, String> }
pub fn tool_bin_dir(fields) -> Result<String>   // only bin_dir is consumed
```
- `ScriptTarget{command, env, working_dir_rel}` becomes a single `BuildAction{id:"script", type: Command,…}`.
- `GroupTarget{member_ids}`.

**`packages/tddy-build/proto/tddy/build/v1/actions.proto`**
```proto
message BuildAction { string id=1; string description=2; ActionType type=3; repeated string command=4;
  map<string,string> env=5; repeated FileSet inputs=6; repeated OutputDecl outputs=7;
  repeated string tool_dep_ids=8;  // ToolTarget ids whose bin_dirs go on PATH
  string working_dir=9; }
enum ActionType { UNSPECIFIED=0; COMMAND=1; COPY=2; TOOL=3; /* invoke a ToolTarget's named command */ }
message FileSet { repeated string include=1; repeated string exclude=2; string root=3; }
```
- `ACTION_TYPE_TOOL` and `COPY` are declared, but the executor runs every action as a command.
- `commands` on a tool is dead.

**`packages/tddy-build/src/lower.rs`**
- `script` → `script_action`; `tool`/`group` → no actions; Test/Run on built-ins → error; anything else → `PluginRegistry.get(type).lower_mode(ctx, mode)`.

**`packages/tddy-build/src/executor.rs`**
- `run_action`:
  - creates the output parent dirs;
  - `spec = build_action_to_spec(repo_root, action)`; the working dir defaults to repo_root;
  - `if let Some(path) = tool_path(...) { spec.env.insert("PATH", path) }`;
  - `ProcessRuntime::spawn(&TaskRegistry::new(), spec, "tddy-build")`, then waits and reads the stdout/stderr channel replay.
- `tool_path`: for each `action.tool_dep_ids` that resolves to a `config.type == "tool"` target, it pushes `repo_root.join(bin_dir)` and then appends the host `$PATH`.
- No sandbox is ever attached.

**`packages/tddy-build/src/action_convert.rs`**
- `build_action_to_spec(repo_root, &BuildAction)` maps proto fields into `BuildActionFields` and calls `tddy_actions::build_action_fields_to_spec`.
- `tool_dep_ids` is not carried; PATH is injected afterwards by the executor.

**`packages/tddy-build/src/capabilities.rs`**
- `BuildMode{Compile, Test, Run}`.
- `ResolvedMeta{tags, languages, capabilities}`.
- `derive_from_type`: `rust_binary` = app, rust, c/t/r; `rust_library` = library, c/t; `typescript` = c/t/r; `docker_image|buildroot_image|qemu_disk_image` = compile only; `script` = compile only; `tool`/`group` = empty.
- `resolve_target_metadata`: declared values win.

**`packages/tddy-build/src/plugin.rs`**
- `trait BuildPlugin { fn type_names(); fn lower(&LowerContext); fn lower_mode(ctx, mode) }`, where `lower_mode` only supports Compile by default.
- `LowerContext{type_name, target_id, target_name, deps, config}`.
- `PluginRegistry` maps type → `Arc<dyn BuildPlugin>`; the last registration wins.

**`packages/tddy-build/src/discovery.rs`**
- Globs `**/BUILD.yaml|BUILD.yml|build.yaml|build.yml` under the repo root.

**`packages/tddy-build/src/service.rs`**
- `build_list_json`, `build_json`, shared by the local CLI and the relay.

**`packages/tddy-build/docs/architecture.md`**
- The "Not yet implemented" list includes "hermetic sandboxing".

**`packages/tddy-build/examples/pipeline/app/BUILD.yaml`** (the only `type: tool` in the repo)
```yaml
  - id: "tools:bin"
    config: { type: tool, bin_dir: tools/bin, commands: { stamp: stamp } }
  - id: "app:build"
    actions:
      - id: "package"
        type: command
        command: ["sh", "-c", "stamp < lib.txt > app.txt"]
        tool_dep_ids: ["tools:bin"]
```

#### tddy-tools / tddy-tools-cli

**`packages/tddy-tools-cli/src/main.rs:30-112`**
- Subcommands: `Submit`, `Ask`, `SpawnConversation*`, `ListTools`, `CallTool`, `Transition`, `GetSchema`, `ListSchemas`, `SetSessionContext`, `PersistChangesetWorkflow`, **`ListActions`**, **`InvokeAction`**, **`BuildList`**, **`Build`** (via `tddy_bsp::build_cli`), **`PtyRelay`**, `Remote`, `SessionHook`, `Notify`, `ListModels`, `Analyze`, `Restructure`, `Extensions`. `--mcp` runs an MCP server.
- There is no `action`/`StartAction` subcommand and no client of `actions.ActionService`.

**`packages/tddy-tools/src/cli.rs:395-405`**
- `run_invoke_action`: if `TDDY_SOCKET` is set, `relay_invoke_action(socket)`; otherwise it runs locally with `session_actions_cli::run_invoke_action(session_dir, …)`. `list-actions` works the same way.

**`packages/tddy-tools/src/session_actions_cli.rs`**
- The local fallback calls `tddy_core::session_actions::{list_actions_in_session_dir, invoke_action_in_session_dir}`.

**`packages/tddy-tools/src/action_tools.rs`**
- MCP tools `request_action` (an attached agent authors the YAML, `prevalidate_manifest_yaml`, then `EstablishAction` relayed to the host), `list_actions` and `invoke_action`. The last two dispatch `"ListActions"`/`"InvokeAction"` through `session_tool_client`.
- Trust note: "this module runs in the jail… The manifest is re-validated and written host-side by the `EstablishAction` relay handler (`tddy-sandbox-app::host_actions`)".

**`packages/tddy-sandbox-app/src/host_actions.rs`**
- Host-side `establish_action`, list and invoke.
- Writes `<session_dir>/actions/<id>.yaml`, which is idempotent; a same-id manifest with different content is a collision error.

**`packages/tddy-bsp/src/build_cli.rs:75-178`**
- `build`/`build-list` relay via `TDDY_SOCKET` (`dispatch_toolcall`); otherwise they run `tddy_build::service::build_json` locally.

**`packages/tddy-tools/src/pty_relay.rs`**
- `pty-relay -- <cmd>` spawns a local PTY.
- `--daemon-url … --sandbox` calls daemon `StartSession` with `sandbox = true` (the claude-cli session path, not actions).

#### Session actions: the one YAML action spec on disk at runtime

**`packages/tddy-session-store/src/session_actions/manifest.rs`**
```rust
#[serde(deny_unknown_fields)]
pub struct ActionManifest { pub version: u32, pub id: String, pub summary: String, pub architecture: String,
  pub command: Vec<String>, #[serde(default)] pub input_schema: Option<Value>, #[serde(default)] pub output_schema: Option<Value>,
  #[serde(default)] pub result_kind: Option<String>, #[serde(default)] pub output_path_arg: Option<String> }
```
- Parsed with `serde_yaml`.

**`packages/tddy-session-store/src/session_actions/paths.rs`**
- Lookup order:
  1. a per-repo store `<tddy_data_dir>/actions/<repo_key>/<id>.{yaml,yml}` (`repo_key` = repo path with `/` → `--`);
  2. a session overlay `<session_dir>/actions/<id>.{yaml,yml}`.
- An `id` may contain `/`, but `..` and `.` segments are rejected.

**`packages/tddy-session-store/src/session_actions/runtime.rs`**
- `action_manifest_to_spec` → `action_spec_from_session_manifest`.
- `run_manifest_blocking` / `start_manifest_async` call `ProcessRuntime::spawn`, with a per-session `TaskRegistry` held in a static `DashMap`.
- **They never sandbox.**

**`packages/tddy-session-store/src/session_actions/arch.rs`**
- `ensure_action_architecture`: "native" or a match against the host arch.

**`docs/ft/coder/session-actions.md`**
- "`command` executes with host permissions of the invoking user".
- `command` has no templating from JSON yet.
- Exit codes: 0, 3 for validation, 1 otherwise.

#### tddy-task

**`packages/tddy-task/src/task.rs`**
- `TaskId` (UUIDv7).
- `TaskStatus{Pending, Running, Completed{exit_code}, Failed{message}, Cancelled}`.
- `ChannelKind{Stdout, Stderr, Combined, Pty /*label only; master in daemon PtyRegistry*/}`.
- `TaskChannel{channel_id, name, kind, broadcast output, TerminalCapture replay, optional stdin mpsc, AppliedOffset ack}` with constructors `new`, `output_only` and `pty`.
- `TaskHandle{id, session_id, kind, created_unix_ms, status, channels, cancel, pid_slot, result_json}`.
- `TaskContext{cancel_token, register_child_pid, deregister_child_pid, pid_slot, channel, set_result, task_id}`.
- `#[async_trait] pub trait TaskBody: Send + 'static { async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus; }`.

**`packages/tddy-task/src/registry.rs`**
- `spawn(body, kind, session_id, channels)` inserts the handle before running the body, plus an `exit_monitor`.
- `create_terminal_task` records a task that is already finished.
- `cancel_task` cancels the token, then escalates with SIGTERM/SIGKILL to the PIDs in `pid_slot`.
- Retention is a 5-minute TTL and a 200-task cap.
- `subscribe_list` / `list_and_subscribe` broadcast events.

#### Other tool catalogs (not declarative specs)

**`packages/tddy-tool-engine/src/catalog.rs`**
- `ToolDef{name, description, input_schema_json}` with `tool_catalog()`, a hard-coded list of MCP exec tool definitions (Read, Shell, …). It has nothing to do with ActionSpec.

#### Docs

- **`docs/ft/daemon/background-tasks.md`** (Status "State B — unified actions runtime"):
  - Lists `PtyRuntime` as a tddy-actions runtime.
  - Says `ListActionKinds` returns "Built-in kinds plus session-scoped manifests" (the code returns only built-ins).
  - Says `StartAction` can "Spawn by kind or inline spec" (no inline spec is supported).
  - Says `GetAction` returns a "channel manifest" (it doesn't).
  - The sandbox table matches the code.
- **`docs/ft/coder/sandbox-builder.md`** line 10: "`ActionSpec` sandbox metadata; sandbox crates do not import `tddy-actions`". Line 73: step 9, Action sandbox plans.
- **`docs/ft/build/tddy-build.md`** line 54: `tool` means "no build action — registers its `bin_dir` on the `PATH` of dependents". Line 172: no hermetic sandboxing.
- **`packages/tddy-daemon-sandbox/docs/task-and-action-services.md`**: the service table, plus known macOS test failures.
- 2026-06-29 changesets:
  - `docs/dev/changesets/2026-06-29-unified-actions-tasks-with-optional-sandbox-execution.md`
  - `docs/ft/daemon/changelog/2026-06-29-…`
  - `packages/tddy-sandbox/docs/changesets/2026-06-29-action-sandbox-plans.md`
  - `packages/tddy-daemon/docs/changesets/2026-06-29-unified-actions-sandbox-action-execution.md`
  - `packages/tddy-build/docs/changesets/2026-06-29-executor-delegates-to-tddy-actions-processruntime.md`
  - `packages/tddy-task/docs/changesets/2026-06-29-pty-channel-kind-for-action-tasks.md`
- Docs that mention it which I did not open: `packages/tddy-service/docs/changesets/2026-06-29-actions-proto.md`, `packages/tddy-core/docs/changesets/2026-06-29-session-actions-unified-runtime.md`, `docs/ft/coder/changelog/2026-06-29-unified-session-actions-via-tddy-actions.md`, `docs/ft/build/changelog/2026-06-29-unified-action-execution-via-tddy-actions.md`, `docs/dev/todo/2026-08-02-unprivileged-userns-…md`, `docs/dev/changesets/2026-09-10-unbundle-sandbox-spawn-services.md`.

---

#### 3. Grep / glob patterns and hits

| Pattern | Scope | Notable hits |
|---|---|---|
| `git ls-files` with `\.action\.ya?ml\|/actions/\|/tools/.*\.ya?ml\|catalog\|\.tool\.ya?ml\|BUILD\.ya?ml` | repo | No `*.action.yaml` or `*.tool.yaml` files and no `actions/` dirs. Only BUILD.yaml files, plus catalog modules unrelated to actions (`tddy-session-catalog`, `tddy-daemon-rpc/src/catalog*`, `tddy-coding-subagent/src/catalog.rs`, `tddy-discovery/src/catalog_service.rs`). |
| all `*.ya?ml` in git | repo | About 40 `BUILD.yaml` files, `config.example.yaml`, `dev.*.yaml`, `sandbox-config.example.yaml`, buf configs. No action or tool spec YAML. |
| `tddy-actions` | `**/Cargo.toml` | root workspace `:83`; `tddy-daemon-sandbox:22`; `tddy-build:25`; `tddy-daemon:64`; `tddy-session-store:29` |
| `tddy_actions` | `*.rs` outside the crate | `tddy-daemon-sandbox/src/{sandbox_action.rs:9, sandbox_plan_builder.rs:6, action_service.rs:8-11, sandbox_runtime.rs:6}`; `tddy-daemon-sandbox/tests/{action_sandbox_acceptance.rs:16-17, action_service_acceptance.rs:8}`; `tddy-build/src/{executor.rs:6, action_convert.rs:6}`; `tddy-daemon/src/runtime/mount_entries.rs:246`; `tddy-session-store/src/session_actions/runtime.rs:9` |
| `PtyRuntime\|SandboxRequest\|spec.sandbox\|extra_read_paths` | packages `*.rs` | `SandboxRequest` appears only in tddy-actions and tddy-daemon-sandbox. `PtyRuntime` is in `tddy-pty/src/runtime.rs:64`. `sandbox: Some(true)` in `tddy-agent-launch` refers to a different (session) spec. |
| `PipelineRuntime\|pipeline-action\|PipelineSpec` | packages `*.rs` | **No production consumer**: only tddy-actions itself and its test, plus the `kind_metadata` string. |
| `StartAction\|ActionServiceClient\|ListActionKinds` | git, excluding md and generated code | **No callers** outside tddy-daemon-sandbox and the proto. |
| `ActionService\|actions_pb` | tddy-web/TS | only generated `packages/tddy-web/src/gen/actions_pb.ts` |
| `type: tool` | yaml/md | only `packages/tddy-build/examples/pipeline/app/BUILD.yaml:6` |
| `tool_dep_ids` | tddy-build | `cache.rs:87` (part of the cache key), `executor.rs:262,266` |
| `build_runner_plan\|tddy_sandbox_recipes::` | `*.rs` | `tddy-daemon-sandbox/src/{sandbox_plan_builder,sandbox_session,workspace_tool_sandbox}.rs`, `tddy-sandbox-app/src/{host_agent,main,spawn}.rs`, `tddy-session-split/src/{agent_argv,split_session}.rs`, `tddy-semantic-index`, `tddy-sandbox-runner/src/runner.rs` |
| `ActionSpec\|action spec\|tddy-actions` | docs | the doc list in section 2 |
| `TDDY_SOCKET` | tddy-bsp `build_cli`, tddy-tools `cli.rs` | relay for build, build-list, invoke-action and list-actions |
| `#[arg(long` | `tddy-sandbox-runner/src/runner.rs` | `SandboxRunnerArgs` fields (section 2) |

---

#### 4. Findings

#### A. What exists today for declaring a runnable tool and its sandbox in YAML
1. **There is no YAML (or JSON file) format for `ActionSpec`.**
   - `ActionSpec` has serde derives, but nothing loads it from disk.
   - Specs are built in Rust from three places:
     - session `ActionManifest` YAML (via `action_spec_from_session_manifest`);
     - tddy-build `BuildAction` (via `build_action_to_spec`);
     - `StartAction` `params_json` (per-kind builders in `action_service.rs`).
   - There is no catalog on disk. `ActionCatalog` is a hard-coded list of 7 kind strings.
2. **The two YAML formats that exist:**
   - **`BUILD.yaml`** (tddy-build). A Bazel-like `targets[]` list with an open `config.type`.
     - The built-in `tool` type is `{bin_dir, commands}`, but only `bin_dir` is used. It is prepended to the PATH of actions that list the tool in `tool_dep_ids`.
     - `ActionType::TOOL` ("invoke a ToolTarget's named command") is declared in the proto but not implemented.
     - There are no sandbox fields, and the docs list "hermetic sandboxing" as not implemented.
     - Discovery is `**/{BUILD,build}.{yaml,yml}`.
   - **Session action manifests**. `actions/<id>.yaml` holds `{version, id, summary, architecture, command, input_schema, output_schema, result_kind, output_path_arg}` with `deny_unknown_fields`.
     - Lookup goes to `<tddy_data_dir>/actions/<repo_key>/` first, then `<session_dir>/actions/`.
     - Manifests are authored by an agent through `request_action`/`EstablishAction`.
     - There are no sandbox, env, cwd or channel fields. They always run unconfined on the host via `ProcessRuntime` with the invoking user's permissions.
3. **How sandboxing is declared on an `ActionSpec`.** `sandbox: Option<SandboxRequest{output_dir, extra_read_paths, recipe, stdin}>`, plus `inputs[]` used as mounts (`host_path`, `jail_path`, `writable`). It is attached only through the `actions.ActionService.StartAction` RPC (`sandbox=true` plus `params.output_dir`, `sandbox_recipe`, `extra_read_paths`, `stdin`).
4. **What the runtime does with it** (`tddy-daemon-sandbox`):
   - It lays out `output_dir/sandbox/<id>/{.work/home, .work/tmp, sandbox.sb}`.
   - Non-PTY modes: `build_process_plan` runs the command directly in the jail, using the recipe's reads, policy and copies; stdout/stderr come from egress-log files polled every 50ms.
   - PTY mode: `tddy-sandbox-runner --pty-command=…` under recipe `RunnerPty`, with a ready marker, gRPC/UDS and `run_host_relay(NullToolHandler)`. PTY bytes go to the task channel and stdin is forwarded.
   - Platform backends: darwin Seatbelt, Linux cgroups/userns, or QEMU by env.
   - There is no fallback on unsupported hosts.
5. **Recipes are Rust enums, not data.**
   - `SandboxRecipe` has 7 variants. Everything per agent is hard-coded per variant:
     - reads (`process_claude_exec_reads`, …);
     - `PolicySpec` (JIT, fork, mach, sysctl, pty, exec_paths);
     - credential copies (Claude, Cursor);
     - the OAuth token secret;
     - OAuth inbound networking;
     - the route config env var.
   - Unknown recipe names silently become `Generic`, including `tddy-coder` and `build-action`, which the `SandboxRequest.recipe` doc comment lists as valid.
6. The `tddy-sandbox` `SandboxPlan` vocabulary is already shaped like a declarative spec:
   - reads with kind, exec, reason and jail remap;
   - mounts (rw/ro, remap);
   - copies (optional, mode);
   - symlinks;
   - env plus secrets (Value or HostFile, delivered out-of-band);
   - policy; network (loopback ports, oauth inbound); limits (memory, cpu, pids); cgroup; stdin.
   - It has no serde derives, though, and is reachable only through Rust builders.

#### B. Consumers
- **tddy-build** `executor.rs`: every build action runs through `ProcessRuntime` (unsandboxed, on a throwaway `TaskRegistry`).
- **tddy-session-store** `session_actions::runtime`: session-action sync and async jobs run through `ProcessRuntime` (unsandboxed, one registry per session).
- **tddy-daemon-sandbox** (re-exported by tddy-session-lifecycle, mounted by tddy-daemon `mount_entries.rs` as `actions.ActionService`): `ActionServiceImpl`, `SandboxRuntime`, `sandbox_plan_builder`, `sandbox_action`. This is the only place sandboxed actions exist.
  - StartAction supports `bash`, `tddy-coder` and `session-action`.
  - `claude-cli`, `build-action`, `pipeline-action` and `execute_tool` are listed but rejected.
- **No external client** calls `actions.ActionService`: not tddy-tools, not the web UI, not the CLI. Only its acceptance tests do.
- `PipelineRuntime` has no production consumer, although the session-actions doc claims it is used for jobs.
- The coding-agent sessions (Claude, Cursor, OpenCode, Muse) **do not use ActionSpec at all**.
  - They build `tddy-sandbox-runner` argv (`--agent-kind`, `--claude-binary`, `--agent-binary`, `--claude-arg`, `--agent-arg`, `--permission-mode`, `--resume`, `--append-system-prompt-file`, `--model`, `--stdio`, …).
  - They go through `SandboxRunnerSpawn` → `build_sandbox_plan` → `build_runner_plan(recipe: None)` and infer the recipe from the argv. That code is in `tddy-daemon-sandbox/src/sandbox_session.rs`, `tddy-session-split`, `tddy-agent-launch` and `tddy-sandbox-app`.
  - The unconfined PTY path uses `tddy-pty::PtyRuntime(PtySpawnSpec)`.
  - So there are about three separate spawn descriptors: `ActionSpec`, `PtySpawnSpec`, and runner argv plus `SandboxRunnerSpawn`.

#### C. Where tddy-tools exposes actions
- `list-actions`/`invoke-action` (session manifests): relayed over `TDDY_SOCKET` to the session owner, or run locally with `--session-dir`.
- MCP tools `request_action`/`list_actions`/`invoke_action` go host-side through `tddy-sandbox-app::host_actions`.
- `build`/`build-list` go through `tddy-bsp::build_cli`, relayed over `TDDY_SOCKET` or run locally with `tddy_build::service`.
- `pty-relay`: a local PTY, or daemon `StartSession{sandbox}` (not actions).
- There is no generic `action run` subcommand, and no CLI path runs an action inside tddy-sandbox. Sandboxed actions are daemon-RPC only.

#### D. tddy-task, the abstraction actions become
- `TaskBody::run(ctx) -> TaskStatus`, registered via `TaskRegistry::spawn(body, kind, session_id, channels)`.
- Channels are named byte streams with replay (`TerminalCapture`), optional stdin and an ack offset.
- `ChannelKind::Pty` is only a label; the real PTY master lives in `tddy-pty::PtyRegistry`.
- Cancellation is a token plus SIGTERM/SIGKILL escalation over `pid_slot`. The result is an opaque `result_json` string.
- Retention is a 5-minute TTL and a 200-task cap.
- There is no structured protocol channel (for example JSON-RPC/ACP), no resize in the task model (that is PtyRegistry), and no readiness or health state beyond Pending/Running.

#### E. Gaps for declaring a long-running interactive coding-agent process
- **PTY**: `ChannelMode::Pty` exists, but:
  - unsandboxed `ProcessRuntime` does not allocate a PTY (pipes only; stdin receiver dropped);
  - sandboxed PTY goes through the runner's `--pty-command` with a fixed `RunnerPty` recipe, which ignores `SandboxRequest.recipe`, so Claude/Cursor policies and credentials are never applied;
  - nothing declares initial size or resize; the runner has `--initial-cols`/`--initial-rows`, but they are not wired from the spec.
- **stdio / ACP protocol**: no field declares a protocol, such as ACP JSON-RPC over stdio, MCP, or the runner's gRPC `SessionChannel`. `tddy-acp-stub` exists only as a BUILD target. The runner's agent kinds are claude, cursor, opencode and muse; ACP is not a runner kind.
- **Streaming**: `ProcessRuntime` buffers until exit, and sandboxed non-PTY mode polls log files. There is no live bidirectional stdio streaming for a long-running process.
- **Env**:
  - `ActionSpec.env` is a plain map. Unsandboxed runs inherit the host env; pipeline stages clear it.
  - Secrets have no spec field. They exist only in `SandboxPlan.env.secrets`, filled by hard-coded recipe logic or by `HarnessRoute`.
- **Mounts**: `inputs[]` (host, jail, writable) maps to `MountSpec`, and `extra_read_paths` maps to `ReadSpec::subpath`. There is no way to declare copies, symlinks, exec grants, read kinds, or a "persistent home" mount.
- **Network**: no spec field. Loopback ports are chosen by the plan builder; `allow_oauth_inbound` is decided by recipe and routing. There is no egress policy, and the egress shim port is implicit.
- **Credentials**: hard-coded per recipe (`claude_credentials_copies`, `cursor_credentials_copies`, `CLAUDE_CODE_OAUTH_TOKEN`), plus provider-route secrets and files. A spec cannot declare them.
- **Home dir**: always scratch `output_dir/sandbox/<id>/.work/home` through `scratch_runner_env`. The session path's persistent claude home is a mount handled outside ActionSpec. There is no declarative home strategy (ephemeral, persistent, seeded).
- **Policy and limits**: `PolicySpec` (JIT, fork, mach, pty, exec_paths) and `ResourceLimits` cannot be set from the spec; they come from recipe enums and daemon config.
- **Lifecycle**: no readiness probe, resume or session-id, model, or permission-mode fields. These exist only as runner flags. Cancellation exists, but there is no restart, attach or reconnect semantics in ActionSpec.
- **Tool provisioning**: the build `tool` type's `bin_dir` maps to PATH only for build actions, and `commands` is unused. Nothing links "tool = binary + its exec reads" to the sandbox side. The sandbox computes reads from `command[0]` via `binary_exec_reads`, or per recipe from the agent binary.

#### F. Doc and code drift worth recording
- tddy-actions changeset and `background-tasks.md`:
  - list `PtyRuntime` and `ActionKind` in tddy-actions (absent);
  - say `ListActionKinds` includes session manifests (it doesn't);
  - say `StartAction` accepts an inline spec (it doesn't);
  - say `GetAction` returns a channel manifest (it doesn't).
- `tddy-build` Cargo description and `architecture.md`: say "no tddy-* deps" (it depends on tddy-actions and tddy-task).
- `SandboxRequest.recipe` doc lists `tddy-coder`/`build-action`, which parse to Generic.
- `ActionInput.jail_path` doc says it defaults to the basename; the code defaults to the full host path.
- `build_action_fields_to_spec` stores glob patterns as literal `host_path`s, so they get mounted as non-existent paths if sandboxed.
- `background-tasks.md` cites the acceptance test path as `packages/tddy-session-lifecycle/tests/action_sandbox_acceptance.rs`; it actually lives in `packages/tddy-daemon-sandbox/tests/`.


### Pass 2 — sandbox runtime configuration (tddy-sandbox*, runner, recipes, daemon-sandbox)

#### Discovery dump: how sandboxes are configured today (worktree `redesign-coding-agents`)

All paths are relative to `/Users/mantasi/Code/tddy-coder/.worktrees/redesign-coding-agents/`.

#### 1. Sequence (chronological)

1. `ls packages | grep sandbox|spawn|pty|agent-launch|daemon` plus `find … | xargs wc -l` for each target crate. Inventory and file sizes.
2. Read `packages/tddy-sandbox/src/{lib.rs,spec.rs,runner_env.rs,tool_ipc.rs,error.rs}`. Core abstraction.
3. Read `packages/tddy-sandbox/src/builder.rs` in full. `SandboxBuilder` / `SandboxPlan` types.
4. Read `packages/tddy-sandbox/docs/architecture.md`, `exec_reads.rs` (head) and `materialize.rs` (head). Architecture, read detectors, secrets.
5. Grepped `context_dir.rs` for `pub fn|claude|TDDY_`, then read lines 1–60. The preamble that tells the agent where its codebase is.
6. Read `packages/tddy-sandbox-darwin/src/{lib.rs,spawn.rs}` and grepped `profile.rs` for `fn|allow|deny|network`.
7. Read `profile.rs` lines 25–215 (`render_plan`, `context_dir_from_runner_argv`, network).
8. Read `packages/tddy-sandbox-recipes/src/{lib.rs,opencode_cli.rs,muse_cli.rs,route_files.rs,plan.rs}`.
9. Grepped the functions in `claude_cli.rs` and `cursor_cli.rs`.
10. Read `claude_cli.rs` lines 1–421.
11. Read `cursor_cli.rs` lines 1–323.
12. Read `packages/tddy-sandbox-runner/src/{lib.rs,main.rs}` and `SandboxRunnerArgs` in `runner.rs` (around lines 268–468).
13. Grepped `runner.rs` for `env(|HTTPS_PROXY|TDDY_|--model|mcp_env|resolve_secret`.
14. Read `runner.rs` lines 1060–1140, 1244–1330 and 1480–1730 (secrets, generic PTY, claude PTY, cursor PTY).
15. Read `runner.rs` lines 1770–1810 and 2384–2640 (egress proxy env, main dispatch, transports).
16. Read `harness_pty.rs`, plus a grep of the RPC allowlist at the top of `runner.rs`.
17. Tried `grep --include=*.rs` (failed under zsh), then located `build_claude_base_argv` in `packages/tddy-agent-backend/src/claude_argv.rs` and read it. Also read `conversation_root.rs` lines 1–60.
18. Read `packages/tddy-daemon-sandbox/src/{lib.rs,sandbox_runtime.rs,sandbox_plan_builder.rs}`.
19. Read `ActionSpec`, `SandboxRequest` and `ActionInput` in `packages/tddy-actions/src/spec.rs`.
20. Grepped the structure of `sandbox_session.rs`, then read lines 230–345 and 490–950 (env builders, plan, persistent home, backend dispatch).
21. Read `packages/tddy-agent-launch/src/svc_start_sandboxed_claude_cli_session.rs` in full.
22. Read `jail_launch_steps.rs` and listed the jail-related files.
23. Read `jail_env_builders.rs` and `jail_session_files.rs`.
24. Grepped `tddy-daemon-kernel` for `resolve_claude_home_dir` etc., then read `config/agent_cli.rs` lines 100–230 and 370–440, and `config.rs` lines 260–290.
25. Read `cli_harness_jail.rs`, and grepped the runner argv, mounts and env in `svc_start_sandboxed_cli_harness_session.rs` and `svc_start_sandboxed_cursor_cli_session.rs`.
26. Read harness-start lines 190–290 and cursor-start lines 310–345.
27. Read the header of `workspace_tool_sandbox.rs` and lines 140–290 (`build_workspace_tool_plan`, ripgrep FIXME).
28. Grepped `sandbox_action.rs` and read lines 1–40.
29. Read `HostRelayConfig` and the tunnel code in `host_relay.rs` (lines 246–290 and 870–1000).
30. Read the cgroups `lib.rs` header, then grepped it and read `spawn_plan` (lines 75–190).
31. Read the qemu `lib.rs`, `argv.rs` (lines 1–80) and `docs/guest-image-9p-init.md` (head).
32. Read the sandbox-app `lib.rs` and `config.rs` in full (`SandboxAppConfig`).
33. Grepped sandbox-app `spawn.rs`, then read lines 517–660 and 700–800.
34. Grepped `tddy-spawn` and `tddy-pty` for `sandbox`, and read their `lib.rs` files and `spawning-as-another-user.md`.
35. Read the `supervisor_client.rs` header, `PtySpawnSpec`, and the `agent_def` grep in `spawner.rs`.
36. Grepped for consumers of `TDDY_SANDBOX_*` (the `--include` form failed, re-ran it as a pipe).
37. Read `tddy-tools/src/server.rs` (around lines 1740 and 3430) and `tddy-session-tool-client/src/lib.rs` lines 240–260.
38. Read sandbox-app `host_agent.rs` lines 60–110 and grepped `codebase_mode.rs`.
39. Ran `grep -li sandbox docs/dev/todo/*` (83 hits out of 400 files) and `find docs/ft -iname '*sandbox*'`.
40. Extracted the title and hit count of every sandbox todo.
41. Read todos: grep-unreachable, lifecycle-shared-jail-launch, warp-cli-sandbox, and sandbox-app-cannot-start-opencode/muse.
42. Read todos: 2026-07-03 sandbox-app/runner, lsp-tools-in-jail, cursor-cli-cannot-enforce, sandboxed-main-needs-session-role, daemon-orphans-sandbox-children.
43. Grepped for `ToolSpec|tools/*.yaml`; read `docs/ft/coder/sandbox-builder.md` lines 1–80.
44. Grepped `REMOTE_APPENDIX`; read the `docs/ft/daemon/claude-cli-session.md` headings, its sandbox-mode section (lines 67–104) and its provider-route section (lines 133–186).
45. Located and read `HarnessRoute` (`tddy-agent-backend/src/provider_route.rs`) and `CliHarness`/`CliLaunch` (`tddy-agent-backend/src/cli_harness.rs`).
46. Scanned the darwin troubleshooting headings, the daemon-sandbox task-and-action-services headings and the relaunch steps; listed each crate's `docs/`.

#### 2. Inspected files and key excerpts

#### packages/tddy-sandbox (generic abstraction)

**`src/spec.rs` — `SandboxSpec`, the legacy struct, still embedded in the plan:**
```rust
pub struct SandboxSpec {
    pub project_root: PathBuf, pub scratch_dir: PathBuf, pub egress_dir: PathBuf,
    pub allow_read_paths: Vec<PathBuf>, pub command: Vec<String>,
    pub env: BTreeMap<String, String>, pub profile_path: PathBuf,
    pub loopback_allow_ports: Vec<u16>, pub ipc_socket: Option<PathBuf>, pub cwd: Option<PathBuf>,
}
```
- `short_ipc_socket_path(session_id)` returns `tmp/tddy-<10 alnum>-<pid>.sock`, which works around the macOS 104-byte `SUN_LEN` limit.
- `SandboxHandle { child, profile_path, grpc_socket_path, ready_marker_path }` provides `take_stdio()` and `try_exit_diagnostic()`. The diagnostic carries a SIGABRT/dyld hint.

**`src/builder.rs` — the plan types, verbatim shapes:**
```rust
pub enum ReadKind { Subpath, Literal, Regex(String), Metadata }
pub enum ReadReason { DyldRoot, SystemLibs, Toolchain, BinaryDeps, OsCaches, UserConfig, Pty, Custom }
pub struct ReadSpec { pub host: PathBuf, pub jail: Option<PathBuf>, pub kind: ReadKind, pub exec: bool, pub reason: ReadReason }
pub struct CopySpec { pub src: PathBuf, pub dest: PathBuf, pub optional: bool, pub mode: Option<u32> }
pub struct SymlinkSpec { pub link: PathBuf, pub target: PathBuf }
pub struct MountSpec { pub host: PathBuf, pub jail: Option<PathBuf>, pub writable: bool }
pub struct SecretSpec { pub env_name: String, pub source: SecretSource }
pub enum SecretSource { Value(String), HostFile(PathBuf) }   // Debug redacts Value
pub struct EnvSpec { pub vars: BTreeMap<String,String>, pub secrets: Vec<SecretSpec> }
pub enum MachPolicy { All, Names(Vec<String>) }
pub struct PolicySpec { allow_dynamic_code_generation, allow_process_fork, mach_lookup: MachPolicy,
                        sysctl_read, pseudo_tty, exec_paths: Vec<PathBuf> }
pub struct NetworkSpec {
    pub loopback_allow_ports: Vec<u16>,
    /// Allow inbound on ephemeral loopback ports (Claude OAuth callback server).
    pub allow_oauth_inbound: bool,
}
pub struct ResourceLimits { memory_max: Option<u64>, cpu_max: Option<String>, pids_max: Option<u64> }
pub struct CgroupConfig { base_override, mount_root, controllers: Vec<String>, supervisor_leaf }
pub struct SandboxPlan { pub spec: SandboxSpec, reads, mounts, copies, symlinks, env: EnvSpec,
                         policy, network, limits, stdin: Option<Vec<u8>>, cgroup: CgroupConfig }
```
- `SandboxBuilder::new(project_root, scratch_dir, egress_dir, command)` takes chained `.read/.reads/.mount/.copy/.symlink/.env_map/.secret/.policy/.network/.limits/.cwd/.stdin/.ipc_socket/.profile_path` calls.
- `build()` is pure. It:
  - deduplicates reads and drops empty-host reads and reads shadowed by an enclosing subpath;
  - requires copy destinations and symlinks to sit inside `project_root`, `scratch_dir` or `egress_dir`, and mounts to be absolute;
  - adds each secret as `TDDY_SECRET_<NAME>=<scratch>/.secrets/<NAME>` (the path only, never the value);
  - defaults the profile to `project_root/profile.sb`;
  - sets `cgroup` to its default.
- Agent-specific leakage in the generic crate:
  - `NetworkSpec.allow_oauth_inbound`, documented as being for the "Claude OAuth callback";
  - the tests name `CLAUDE_CODE_OAUTH_TOKEN` and `/usr/local/bin/claude`.

**`src/runner_env.rs` — the generic in-jail env:**
```rust
pub fn process_jail_env(scratch_home, scratch_tmp) -> {HOME, TMPDIR, TERM=xterm-256color, PATH=/usr/bin:/bin:/usr/sbin:/sbin}
pub fn scratch_runner_env(scratch_home, scratch_tmp, session_id, tool_ipc_socket, egress_dir)
  = process_jail_env + TDDY_SANDBOX_SESSION_ID, TDDY_SANDBOX_TOOL_IPC, TDDY_SANDBOX_EGRESS_DIR,
    RUST_LOG (host's or "info"), passthrough TDDY_EGRESS_PROBE_{HOST,PORT,URL,TARGET}
```
PATH is fixed. There is no mechanism for putting binaries on it.

**`src/lib.rs`:**
- `workspace_exec_tool_names()` returns `[Read, Write, StrReplace, Delete, Grep, Glob, Shell, Await, ReadLints, SemanticSearch]`, which "must stay in sync with tddy_daemon::tool_catalog". This is tddy-tools vocabulary sitting in the generic crate.
- The `spawn()` facade always returns `Unsupported`.

**`src/tool_ipc.rs`:** `session_id_from_env()` reads `TDDY_SANDBOX_SESSION_ID` and falls back to `TDDY_REMOTE_SESSION_ID`.

**`src/exec_reads.rs`:**
- `system_baseline_reads()` on macOS:
  - `/` literal (dyld);
  - `/usr/lib`, `/usr/libexec` (exec), `/System`, `/Library`, `/private/var/db/dyld`, `/private/etc`;
  - `/usr/bin`, `/bin`, `/sbin` (exec);
  - zoneinfo, ICU;
  - `/dev/*` literals and PTY regexes.
  - It deliberately omits `/private/var/folders`.
- On Linux the baseline is `/usr/bin`, `/bin`, `/usr/lib`, `/lib`, `/lib64`, `/etc/ssl/certs`, `resolv.conf`, `ld.so.cache` and zoneinfo, filtered to those that exist.
- `binary_exec_reads(bin)` grants the parent directory (exec) plus the `otool -L` dependencies.
- `process_exec_reads(bin)` is baseline + `detect_toolchain_reads()` + `binary_exec_reads`.
- `path_traversal_reads(path)` grants ancestors as Subpath (used by the Cursor recipe).

**`src/materialize.rs`:** `materialize_copies`, `materialize_symlinks`, and `materialize_secrets` (secrets go to `scratch/.secrets` with the directory at 0700 and files at 0600).

**`src/context_dir.rs` — where the agent is told it is confined (managed codebase):**
```rust
pub const MANAGED_CODEBASE_PREAMBLE: &str = r#"## Managed Codebase
The real codebase is MANAGED — it is NOT in this local directory. …
You MUST use the `mcp__tddy-tools__*` tools (Read, Write, StrReplace, Delete, Grep, Glob, Shell,
Await, ReadLints, SemanticSearch) for ALL file and shell operations. …"#;
pub const PREAMBLE_FILES: &[&str] = &["CLAUDE.md", "AGENTS.md"];
```
- `SandboxContextDir::create(source, globs)` and `create_with_subagent(…, replacements, globs)` copy the agent-config globs and prepend the preamble.
- `managed_codebase_preamble(replacements)` adds subagent delegation clauses (`subagent_new_session/prompt/await`) and the `request_action/list_actions/invoke_action` paragraph when `Shell` is replaced.
- The crate is generic in name, but this whole module is agent/MCP-specific.

**`docs/architecture.md`** covers:
- the crates table;
- stdio transport;
- the `--workspace-tools` jail;
- the codebase-modes table (`mounted`, `managed`, `sandboxed`);
- the `SandboxSpec` field table;
- the builder sub-spec table;
- the context dir.

It notes "SecretSpec … The runner reads it and sets it on the inner `claude` child only".

#### packages/tddy-sandbox-darwin (Seatbelt)

**`src/spawn.rs`:**
- `sandbox_exec_argv(plan)` returns `["/usr/bin/sandbox-exec","-f",profile,"/usr/bin/env","-i", K=V…, command…]`. The env is cleared via `env -i`, and only `plan.env.vars` are passed.
- `spawn_plan` creates the directories, runs `materialize_*`, renders and writes the profile, then sets cwd (`spec.cwd` or `project_root`).
  - stdin/stdout are piped when argv contains `--stdio`; otherwise stdout goes to an egress log.
  - It calls `setpgid(0,0)` and writes a spawn manifest containing `"egress_via":"session_channel","network_policy":"deny"`.
- `detect_allow_read_paths()` runs `xcode-select -p`, `which node`, `brew --prefix` and `command -v sh`.

**`src/profile.rs` — `render_plan`, in order:**
1. Header comment: "Tight Seatbelt profile for sandboxed Claude Code CLI sessions" (agent-specific wording).
2. `(deny file-write*)`, then `(allow file-write* …)` for the project, scratch and egress dirs plus writable mounts.
3. File-write allows for `/dev` nodes.
4. `(deny file-write* (subpath <context_dir>))`, but only when argv carries `--context-dir`. `context_dir_from_runner_argv` sniffs the runner argv.
5. `(allow file-read* …)` for the writable tree, every non-Metadata read, secret files and mounts.
6. `(allow file-read-metadata …)` for Metadata reads.
7. Policy: dynamic-code-generation, process-fork, mach-lookup (All or names), sysctl-read, pseudo-tty, and a static `file-ioctl` for the PTY.
8. `(allow process-exec* project_root + policy.exec_paths + exec subpath reads + all mounts)`.
9. `render_network`:
```
(deny network*)
(allow network-bind (local unix-socket)) (allow network-inbound (local unix-socket)) (allow network-outbound (remote unix-socket))
[if ports||oauth] (allow network-bind (local tcp "localhost:*"))
[if oauth]        (allow network-inbound (local tcp "localhost:*"))
per port: (allow network-outbound (remote tcp "localhost:{port}")) (allow network-inbound (local tcp "localhost:{port}"))
ipc_socket: (allow file-read*/file-write* (literal …))
```

There is **no host or domain egress policy**: network is all-deny except loopback and UDS.

#### packages/tddy-sandbox-cgroups (Linux)

`spawn_plan`:
- validates, creates the directories, and fails fast unless `unprivileged_userns_available()`;
- runs `materialize_*`;
- reads `--grpc-uds` and `--ready-marker` back from argv;
- resolves the delegated cgroup base (`detect_and_prepare_base(&plan.cgroup)`) and creates the scope `tddy-<name>-<seq>.scope`;
- uses `cmd.env_clear(); cmd.envs(&plan.spec.env)`;
- in `pre_exec`: `unshare(NEWUSER)` with uid mapped to 0, then `NEWNS|NEWNET`, a private root, loopback up, and RO bind mounts from `plan_to_bind_mounts` (`MS_NOEXEC` when `!exec`; Metadata and Regex produce no mount);
- moves the pid into the scope and writes limits. If the plan sets none, defaults apply: `memory_max` 2 GiB, `cpu_max` `"100000 100000"`, `pids_max` 512.

Known gap, `FIXME(fs-confinement)`: there is no `pivot_root`, so the jail still shares the host root.

Network has no interfaces except `lo`, so traffic must go through the in-jail `HTTPS_PROXY`.

#### packages/tddy-sandbox-qemu

- `spawn_plan` / `spawn_plan_with(plan, QemuBackendOptions{image_path, overlay_path, runner_dir_path, control_port})`.
- Each `MountSpec` becomes a virtio-9p fsdev (readonly unless writable).
- `guest_plan_json(plan, opts)` serializes mounts, env, cwd, command and the control port into a `tddy-plan` 9p share.
- The runner binary is injected via the `tddy-runner` 9p tag.
- The guest init script is `S99tddy-sandbox` and needs `jq` (see `docs/guest-image-9p-init.md`).
- The backend is selected by `TDDY_SANDBOX_BACKEND=qemu` (`tddy-daemon-sandbox::sandbox_session::qemu_backend_requested`).
- It consumes only part of the plan: mounts, env, cwd and command. Reads and policy do not apply in a VM.

#### packages/tddy-sandbox-recipes ("product-specific recipes")

**`src/plan.rs`:**
```rust
pub enum SandboxRecipe { Generic, ClaudeCli, CursorCli, Shell, RunnerPty, OpenCodeCli, MuseCli }
pub struct RunnerPlanRequest { project_root, scratch_dir, egress_dir, profile_path, runner_argv: Vec<String>,
    env: BTreeMap, loopback_allow_ports: Vec<u16>, ipc_socket: Option<PathBuf>, mounts: Vec<MountSpec>,
    recipe: Option<SandboxRecipe>, host_home: Option<PathBuf>, route: Option<HarnessRoute> }
pub struct ProcessPlanRequest { project_root, scratch_dir, egress_dir, profile_path, command, env, mounts,
    recipe: SandboxRecipe, host_home, cwd, extra_reads: Vec<ReadSpec>, stdin }
```
- `detect_recipe_from_argv` infers the recipe by sniffing runner argv:
  - `--agent-kind cursor|opencode|muse|<else>=ClaudeCli`;
  - `--claude-binary` gives ClaudeCli;
  - `--pty-command` gives RunnerPty;
  - otherwise Generic.
- `agent_binary_from_runner_argv` reads `--claude-binary`, or `--agent-binary` for cursor, opencode and muse.
- `collect_reads(recipe, runner, agent)`:
  - Claude: `process_claude_exec_reads(claude)`, which is the same as `process_exec_reads`;
  - Cursor: adds install dir, bundled node, `index.js`, the `cursor-agent` share root, and `~/.local/bin` and `~/.local/share` traversal;
  - OpenCode: `process_exec_reads`;
  - Muse: baseline + binary;
  - Generic, Shell and RunnerPty: baseline + primary binary;
  - always: the runner binary's reads.
- Policies:
  - `claude_interactive_policy` is JIT + fork + `mach All` + sysctl + pty, with exec paths `/usr/bin`, `/usr/libexec`, `/bin`, `/sbin`, `/System`, `/Library`, `/Applications`, `/opt/homebrew`, `/opt/local`;
  - cursor and opencode reuse it;
  - muse is the same without JIT;
  - shell is fork + pty, no JIT, `mach Names([])`, with exec paths `/usr/bin`, `/bin`, `/sbin`, `/usr/libexec`, `/System`, `/Library`;
  - `runner_pty` is JIT + fork + `mach All` + sysctl + pty;
  - generic is JIT + fork + `mach All` + sysctl, no pty.
- `copies_for_recipe`:
  - Claude: `claude_credentials_copies(host_home, scratch/home)`;
  - Cursor: `cursor_credentials_copies`;
  - all others: none.
- `finish_builder`: when the recipe is ClaudeCli and the session is not routed, it reads the **daemon's own env** `CLAUDE_CODE_OAUTH_TOKEN` and adds it as `.secret("CLAUDE_CODE_OAUTH_TOKEN", Value)`.
- `build_runner_plan`:
  - when routed, merges `route.env` into the env and runs `materialise_route_files(scratch, route)`, which writes `scratch/provider-route/*` at 0600 and sets the harness's `route_config_env` (`OPENCODE_CONFIG`) to the first file;
  - `NetworkSpec{ loopback_allow_ports, allow_oauth_inbound: !routed && recipe∈{ClaudeCli,CursorCli} }`;
  - `route_secrets()` turns `route.secret_env` into jail secrets.
  - `route.args` is **not** carried into the jail. The runner has no channel for route args; `cli_harness_jail.rs` works around this by lifting the `--model` value out of them.
- `build_process_plan`: direct command with `cwd`, `stdin` and `extra_reads` (used by actions).
- `recipe_from_name`: `claude-cli`, `cursor-cli`, `opencode-cli`, `muse-cli`, `bash|shell`, `generic`, anything else → Generic.

**`src/claude_cli.rs` (everything here is agent-specific):**
- `seed_claude_credentials(claude_home)`: copies host `~/.claude/.credentials.json` into the persistent home once, never overwriting, at 0600.
- `seed_claude_local_install(home, claude_binary)`: creates the symlink `home/.local/bin/claude` → `home/.local/share/claude/versions/<v>` → real binary, so Claude's self-check passes.
- `claude_credentials_copies(host_home, scratch_home)`: a single `CopySpec` for `.claude/.credentials.json` (optional, 0600) for an ephemeral home.
- `claude_runner_env_overlay(tmp)`: `CLAUDE_CODE_TMPDIR`, `CLAUDE_TMPDIR`.
- `PERMISSION_PROMPT_TOOL = "mcp__tddy-tools__approval_prompt"`.
- `SUBAGENT_TOOLS`: `mcp__tddy-tools__subagent_{new_session,prompt,await,resume,cancel,end,diff,pull}`.
- `native_aliases`: `Shell` → `Bash, BashOutput, KillShell`; `Write` → `Edit, MultiEdit, NotebookEdit`.
- `effective_replaced_tools(replaced, semantic_index_enabled)` adds `SemanticSearch` when indexing is off.
- `build_claude_allowlist(subagent_enabled, replaced)` uses `tddy_workflow_recipes::permissions::build_remote_allowlist(remaining exec tools)` and appends the subagent tools.
- `build_claude_disallowlist(replaced)` emits both the native and `mcp__tddy-tools__` forms plus the aliases.
- `write_claude_mcp_config(scratch, tddy_tools_path, mcp_env)` writes `claude-mcp-config.json` as `{"mcpServers":{"tddy-tools":{"command":…,"args":["--mcp"],"env":…}}}`.
- `build_host_agent_disallowlist` (Read, Write, Grep, Glob + aliases) and `append_host_agent_mcp_args` serve sandboxed-codebase mode.
- `append_claude_mcp_args(argv, …)` appends `--allowedTools…`, `--disallowedTools…`, `--permission-prompt-tool mcp__tddy-tools__approval_prompt` and `--mcp-config <path>`.
- `claude_scratch_mcp_dir(fallback)` returns `$TMPDIR` or the fallback.

**`src/cursor_cli.rs`:**
- `seed_cursor_credentials`: copies `~/.cursor/{cli-config.json,auth.json,mcp.json}` once.
- `cursor_credentials_copies`: `auth.json` (0600) and `cli-config.json`.
- `seed_cursor_local_install`.
- `write_cursor_mcp_config(base, …)` writes `<base>/.cursor/mcp.json`.
- `build_cursor_sandbox_argv`: when present, runs `<install>/node <install>/index.js --model … args` directly instead of the wrapper.
- `cursor_runner_env_overlay(tmp)`: `CURSOR_TMPDIR`, `CURSOR_SKIP_KEYCHAIN=1`, `CI=true`.

**`src/muse_cli.rs` / `opencode_cli.rs`:** policies and reads only. Muse carries `TODO(muse-cli-facts)`.

#### packages/tddy-sandbox-runner (in-jail runner and host relay)

**`src/runner.rs` — `SandboxRunnerArgs` (clap), the in-jail exec contract:**
```
--session-id, --context-dir, --cwd?, --grpc-socket? (vestigial), --tool-ipc-socket,
--tddy-tools-path? ("Required for Claude mode"), --claude-binary (default "claude"),
--agent-kind (claude|cursor|opencode|muse, default claude), --agent-binary?, --model (default ""),
--ready-marker, --permission-mode (default auto), --dangerously-skip-permissions, --resume,
--append-system-prompt-file?, --grpc-listen-port?, --egress-shim-port?, --grpc-uds?,
--pty-command (repeatable; generic action mode), --claude-arg (repeatable), --agent-arg (repeatable),
--mcp-log-level?, --stdio, --initial-cols (80), --initial-rows (24)
main.rs adds: --workspace-tools <worktree>
```

`run_sandbox_runner_inner` sequence:
1. Initialise egress logging from `TDDY_SANDBOX_EGRESS_DIR`.
2. Start the egress shim on `127.0.0.1:<--egress-shim-port>` **always**.
3. Start the tool-IPC server on `--tool-ipc-socket`, except in `--pty-command` mode.
4. Set `cwd` to `--cwd`, defaulting to `--context-dir`.
5. Pick the PTY mode:
   - `--pty-command` set: `spawn_generic_pty`. This sets only `TERM` and inherits the jail env. It gets **no** `HTTPS_PROXY` and no secrets.
   - `opencode` or `muse`: `spawn_harness_pty`, with argv from the `tddy_agent_backend::cli_harness` descriptors.
   - `cursor`: `spawn_cursor_pty`. `tddy_tools_path` is required. It writes `.cursor/mcp.json` under `$HOME`, falling back to `cwd`.
   - otherwise Claude: `spawn_claude_pty`. `tddy_tools_path` is required.
6. Serve `SandboxService` over stdio, UDS or loopback gRPC.

**`spawn_claude_pty`** builds argv from `tddy_agent_backend::claude_argv::build_claude_base_argv` (`claude [--model m] (--resume|--session-id) <id> (--dangerously-skip-permissions | --permission-mode p)`), then:
- adds `--append-system-prompt-file`;
- adds the `--claude-arg` passthrough;
- reads env-signalled agent features: `TDDY_SUBAGENT`, `TDDY_SUBAGENTS_JSON` / `TDDY_SUBAGENT_REPLACES` (the replaced set), and `TDDY_SEMANTIC_INDEX_DB` (`semantic_index_enabled`);
- builds `mcp_env = {TDDY_TOOLS_LOG_FILE=<egress>/tddy-tools.mcp.log, TDDY_TOOLS_ACCOUNTING_FILE=<egress>/accounting.json, RUST_LOG=<--mcp-log-level or "info,tddy_tools=debug,tddy_discovery=debug">}`;
- calls `append_claude_mcp_args(... claude_scratch_mcp_dir(context_dir) ...)`.

**`run_claude_pty_thread`** is shared by Claude, Cursor, OpenCode and Muse:
```rust
cmd.env("TERM", "xterm-256color");
cmd.env("TDDY_EGRESS_SHIM", &egress_shim);
// claude honors HTTPS_PROXY and issues `CONNECT api.anthropic.com:443` …
cmd.env("HTTPS_PROXY", &egress_shim); cmd.env("HTTP_PROXY", …); cmd.env("https_proxy", …); cmd.env("http_proxy", …);
// passthrough TDDY_EGRESS_PROBE_*
for (name, value) in resolve_secret_envs(&process_env) { cmd.env(name, value); }  // TDDY_SECRET_<N> → N, file unlinked
```
The PTY child inherits the runner env (HOME, TMPDIR, PATH, TDDY_*, CLAUDE_* …) plus the variables above. No `NO_PROXY` is set on the agent PTY; `egress_proxy_env()`, which does set `NO_PROXY=127.0.0.1,localhost`, is used only for `--workspace-tools` subprocesses.

**`resolve_secret_envs`** turns `TDDY_SECRET_<NAME>=<path>` into `(NAME, contents)` and unlinks the file.

**Egress shim:**
- Handles `CONNECT host:port` and turns it into a `TunnelOpen` frame on the SessionChannel.
- Rewrites plain-HTTP absolute-form requests (`rewrite_http_proxy_request`) and forwards them over a tunnel.
- `GET /probe` uses `TDDY_EGRESS_PROBE_URL/HOST/PORT`.

**Tool IPC** (`ToolExecService` over `tddy-rpc` framing on the UDS):
- serves `exec_tools.ExecToolService/ExecuteTool`, which is queued and delivered to the host on `HostPoll`;
- forwards the RPCs in `tddy_service::session_agents::IN_JAIL_RELAYABLE` (roster, conversation worktree, `ListExecTools`);
- `bind_to_this_session` rewrites the session id so requests cannot name another session.

**`run_workspace_tool_runner`** (`--workspace-tools`): runs no PTY and no MCP. It serves `in_jail_tool_request` through `tddy_tool_engine::execute_tool_with_env`, with `tool_env = egress_proxy_env(shim_port)`. The proxy env is set only if `--egress-shim-port` is given.

**`src/harness_pty.rs`:** `harness_pty_argv(kind, binary, model, None, resume)` uses `cli_harness("opencode-cli"|"muse-cli").argv`.

**`src/conversation_root.rs`:** `CONVERSATION_WORKTREES_DIR = "tmp/subagent-worktrees"`.

**`src/host_relay.rs`:**
- `HostRelayConfig { session_id, terminal_sink, terminal_events, initial_cols, initial_rows }`.
- Traits: `HostToolHandler`, `HostRpcHandler`, `SessionChannelClient`.
- `spawn_tunnel` calls `TcpStream::connect(format!("{}:{}", open.host, open.port))` with **no allowlist or denylist check**: any host and port the agent CONNECTs to is dialled by the host.
- `relay_egress_request` is a reqwest unary fetch.

#### packages/tddy-agent-backend (descriptors the runner uses)

- `claude_argv.rs::build_claude_base_argv(binary, model, session_id, permission_mode, dangerously_skip, resume)`.
- `cli_harness.rs::CliHarness { session_type, label, route_harness, context_agent, default_binary, binary_env, route_config_env, argv: fn(&CliLaunch)->Vec<String>, models }`. Registry entries: claude-cli (`TDDY_CLAUDE_BINARY`), cursor-cli (`agent`, `TDDY_CURSOR_AGENT`), opencode-cli, muse-cli, and one more.
- `CliLaunch { binary, model, session_id, conversation_id, initial_prompt, permission_mode, dangerously_skip_permissions, resume }`.
- `provider_route.rs::HarnessRoute { env: Vec<(String,String)>, secret_env: Vec<(String,RouteSecret)>, files: Vec<RouteFile>, args: Vec<String> }`.

This descriptor registry is the closest existing thing to a declarative per-tool spec: argv, binary env, context globs and route config env. It is Rust statics, not YAML.

#### packages/tddy-daemon-sandbox

**`src/sandbox_session.rs`:**
- `build_sandbox_runner_env(...)` = `scratch_runner_env` + `claude_runner_env_overlay`. It is used for Claude **and** for the opencode/muse harness and relaunch, so `CLAUDE_CODE_TMPDIR` leaks into non-Claude jails.
- `build_sandboxed_cursor_runner_env` = the above + the cursor overlay.
- `prepare_context_dir(_with_subagent)`.
- `resolve_tddy_tools_path(configured)`: config, then `CARGO_BIN_EXE_tddy-tools`, then a sibling of `current_exe`, then `"tddy-tools"`.
- `resolve_sandbox_runner_path()`.
- `build_allow_read_paths(runner_argv)`: toolchain detection, the runner, plus `--claude-binary`, plus the cursor `--agent-binary` (sniffs argv again). On macOS it is merged into the plan in `merge_allow_read_paths_into_plan` as exec subpaths.
- `pick_free_loopback_port()`.
- `terminate_sandbox_process(pid)`.
- The struct:
```rust
pub struct SandboxRunnerSpawn { project_root, scratch_dir, egress_dir, profile_path, runner_argv, env,
  loopback_allow_ports, ipc_socket, mounts: Vec<MountSpec>,
  /// Host `$HOME` used by the recipe to auto-copy `.claude/.credentials.json` … `None` disables …
  host_home: Option<PathBuf>, cgroup: CgroupConfig, route: Option<HarnessRoute> }
```
- `build_sandbox_plan(params)` calls `build_runner_plan(... recipe: None ...)`, so the recipe is inferred from argv; then it sets `plan.cgroup` and, on macOS, merges the allow-read paths.
- `prepare_persistent_claude_home(dir, claude_binary)` creates the directory, then runs `seed_claude_credentials` and `seed_claude_local_install`, and canonicalises the result. `prepare_persistent_cursor_home` is the Cursor equivalent.
- `spawn_sandbox_plan(plan)` dispatches to qemu (when requested), darwin `spawn_plan`, cgroups `spawn_plan`, or `Unsupported`.
- `spawn_sandbox_runner(params)` = `spawn_sandbox_plan(build_sandbox_plan(params))`.
- `DaemonToolHandler` (host side) runs `conversation_tool::execute_in_conversation` with `session_env` (`TDDY_SOCKET`, `PATH`, git identity) and records agent-activity rows.
- `dial_and_bridge` and `bridge_sandbox_stdio`.

**`src/sandbox_plan_builder.rs`** (actions → plans):
- `ActionSandboxLayout::under_output_dir(out, id)` gives `out/sandbox/<id>/{.work, sandbox.sb}` with egress at `out`.
- `input_mounts(ActionInput[])` becomes `MountSpec` (jail path defaults to the host path).
- `build_action_sandbox_plan(spec, layout, sid, extra_env)`:
  - recipe from `sandbox.recipe` or `spec.kind`;
  - env = `scratch_runner_env`, plus `claude_runner_env_overlay` if the recipe is ClaudeCli, plus `extra_env`;
  - calls `build_process_plan` with `host_home = $HOME`, so a claude-cli action copies host credentials.
- `build_action_runner_plan`: recipe `RunnerPty`; two loopback ports (gRPC and shim); runner argv `--session-id --context-dir --cwd --grpc-socket --tool-ipc-socket --ready-marker --grpc-listen-port --egress-shim-port --model "" --pty-command=<each arg>` (plus `--grpc-uds` on Linux); `route: None`.

**tddy-actions `src/spec.rs`** (the only YAML/serde-declarative sandbox request today):
```rust
pub struct ActionSpec { id, kind, command: Vec<String>, inputs: Vec<ActionInput>, outputs, env: BTreeMap,
  working_dir: Option<PathBuf>, channel_mode: ChannelMode, sandbox: Option<SandboxRequest>,
  session: Option<SessionActionExtras>, pipeline: Option<PipelineSpec> }
pub struct SandboxRequest { output_dir: PathBuf, extra_read_paths: Vec<PathBuf>,
  recipe: Option<String> /* claude-cli, bash, tddy-coder, build-action, … */, stdin: Option<String> }
pub struct ActionInput { host_path, jail_path: Option<PathBuf>, writable: bool }
```

**`src/sandbox_action.rs`:** `spawn_sandbox_action` maps `ChannelMode::Pty` to `run_pty_confined_action` (runner plan plus `run_host_relay` with `NullToolHandler`); any other mode maps to `run_confined_plan` (process plan plus an egress log tail). It has its own copy of `spawn_confined_plan` dispatch.

**`src/workspace_tool_sandbox.rs`** (agent-less jail, `workspace` sessions):
- `WorkspaceToolPlanRequest { layout, worktree_path, session_id, runner_path, tddy_tools_path, cgroup }`.
- Runner argv `… --workspace-tools <wt> --stdio`, with no egress shim and `ipc_socket: None`.
- Mount: the worktree rw.
- Recipe: `Shell`.
- Metadata reads for the worktree's ancestors.
- `plan.policy.sysctl_read = true` (the Rust runtime needs it).
- `FIXME(grep-in-jail)`, `host_ripgrep_grants()`: prepends the `rg` directory to PATH and adds exec reads. The doc comment names the "designed replacement": a declared tool set, `<tddyhome>/tools/*.yaml` naming a host binary or a `tddy-build` target, each stating what becomes readable, executable and reachable on `PATH`.

#### packages/tddy-agent-launch (callers for coding sessions)

**`svc_start_sandboxed_claude_cli_session.rs`:**
- Parameter structs `JailSession`, `JailBranch`, `JailDirs{sandbox_root, egress_dir, scratch_dir, scratch_tmp, context_dir}`, `JailLaunch`, `JailRunnerEnv`.
- Flow:
  1. `warm_up_jail_agents`.
  2. Create the session directory, changeset and worktree (a project worktree or `repo_path`).
  3. Claim seed clones.
  4. `prepare_jail_dirs`: `<session>/sandbox/{.work/home,.work/tmp,context}` and `<session>/egress`, canonicalised.
  5. `prepare_jail_context_dir` with the `"claude-cli"` globs and the roster replacement pairs.
  6. Resolve `tddy_tools_path` (`config.claude_cli.tddy_tools_path`).
  7. `managed_jail_env` gives `(ManagedWorkflow, append_system_prompt_file, session_env)`.
  8. Canonicalise the exec paths.
  9. `claude_binary = resolve_claude_binary_path(config)` (`TDDY_CLAUDE_BINARY`, then `claude_cli.binary_path`, then `~/.local/bin/claude` or PATH skipping shims).
  10. `claude_home_dir = resolve_claude_home_dir(config)`; `scratch_home = prepare_persistent_claude_home(...)`.
  11. `tool_ipc_socket = short_ipc_socket_path`.
  12. Pick the egress-shim port; `loopback_allow_ports = [shim]`.
  13. Runner argv:
```
<runner> --session-id <id> --context-dir <ctx> --tool-ipc-socket <sock> --tddy-tools-path <tt>
  --ready-marker <m> --claude-binary <claude> --model <m> --permission-mode <p> --egress-shim-port <port> --stdio
  [--dangerously-skip-permissions] [--append-system-prompt-file <f>] [--claude-arg <tok>]…
```
  14. `jail_runner_env` = `build_sandbox_runner_env` + `specialized_subagent_env` (`TDDY_SUBAGENT`, `TDDY_SUBAGENTS_JSON`) + `TDDY_DAEMON_INSTANCE_ID` + `TDDY_LSP_TOOLS=rust` (if available) + `TDDY_RESTRUCTURE_TOOLS=1` (if available) + `TDDY_SEMANTIC_INDEX_DB` + `config.claude_cli_renderer_env()` (`CLAUDE_FULLSCREEN_ENV=1`).
  15. `launch_jail`.
  16. Write `.session.yaml` with `sandbox: Some(true)` and `managed_codebase`.

**`jail_launch_steps.rs::launch_jail`:**
- Calls `spawn_sandbox_runner(SandboxRunnerSpawn{ …, mounts: vec![MountSpec::read_write(scratch_home)], host_home: None, cgroup: config.sandbox_cgroup_config(), route })`.
- The persistent `.claude` home is the **only** mount. The worktree is never mounted (managed codebase), and `host_home: None` disables the per-session credential copy.
- Then `wait_for_sandbox_ready` (120 s), `dial_and_bridge(... Arc::new(session_env) ..., host.sandbox_rpc_handler)` and `SandboxSessionState` registration.
- `jail_relay_identity` decides the git identity env for the host-side relay and creates a logged-out `GH_CONFIG_DIR` for other users.

**`jail_env_builders.rs`:** `specialized_subagent_env`, `jail_daemon_identity_env`, `restructure_tools_env`, `lsp_tools_env`. Has `TODO(session-agent-roster)` AC25.

**`cli_harness_jail.rs`** (opencode and muse):
- `sandboxed_agent_kind`: `opencode-cli` → `opencode`, `muse-cli` → `muse`, everything else is refused ("the sandbox runner does not host …").
- `require_provider_route`: a route is mandatory because "a jail's `$HOME` is the session's own scratch … holds no credentials".
- `sandboxed_runner_model` lifts `--model` from `route.args`.
- `refuse_first_prompt`.
- `resolve_jail_agent_binary` searches PATH.
- `cli_harness_runner_argv` gives `… --agent-kind <k> --agent-binary <b> --model <m> --egress-shim-port <p> --stdio [--resume]`.

**`svc_start_sandboxed_cli_harness_session.rs`:**
- `scratch_home = <session>/sandbox/.work/home`, which is per-session, not daemon-wide.
- Env is `build_sandbox_runner_env` (this carries the Claude overlay) plus the same overlays.
- Mounts are `[rw scratch_home]` with `host_home: None` and `route: Some(route)`.

**`svc_start_sandboxed_cursor_cli_session.rs`:**
- `prepare_persistent_cursor_home`.
- Argv uses `--agent-kind cursor --agent-binary …`, with the initial prompt passed as `--agent-arg`.
- `build_sandboxed_cursor_runner_env`.
- Mounts are `[rw cursor home]`.

**`relaunch_jail_steps.rs` / `svc_relaunch_sandboxed_runner.rs`:** the same as the Claude start, plus `--resume`.

#### tddy-daemon-kernel config

- `ClaudeCliConfig { binary_path, tddy_tools_path, daemon_url, claude_home_dir, fullscreen, answer_wait_secs }`.
- `resolve_claude_home_dir`: `TDDY_SANDBOX_CLAUDE_HOME`, then `claude_cli.claude_home_dir`, then `$HOME/.tddy/sandbox-claude-home`. It is "deliberately separate from the daemon user's real `~/.claude`".
- `resolve_cursor_home_dir`: `TDDY_SANDBOX_CURSOR_HOME`, then `cursor_cli.cursor_home_dir`, then `$HOME/.tddy/sandbox-cursor-home`.
- `sandbox_cgroup_config()` maps the `sandbox_cgroup:` YAML section.
- `CliHarnessConfig { binary_path }` lives under `cli_harnesses.<type>`.

#### packages/tddy-sandbox-app (standalone)

**`src/config.rs` — `SandboxAppConfig`, verbatim fields (`deny_unknown_fields`):**
```rust
model, permission_mode, codebase_mode ("mounted"/"managed"[/"sandboxed"]), claude_binary, cursor_binary,
tddy_tools_path, sandbox_runner_path, session_base: Option<PathBuf>, claude_home_dir, cursor_home_dir,
codebase_home_dir /* sandboxed mode: base of per-repo build homes */, cwd, agents_dir,
specialized_agents: Vec<String>, subagents: Vec<SpecializedAgentDef>, claude_args: Vec<String>, mcp_log_level
```
- `resolve_session_agents(named, inline, agents_dir)` refuses an agent that both replaces `Shell` and binds `SHELL`.

**`src/codebase_mode.rs`:** `CodebaseMode { Mounted, Managed, Sandboxed }`; `--remote-codebase` is an alias for Managed.

**`src/spawn.rs::spawn_claude_sandbox`:**
- Refuses OpenCode and Muse (`unstarted_agent_kind`, `TODO(sandboxed-routes)`).
- Picks the persistent home by kind (claude or cursor) and runs its own crate-local copies of `seed_claude_credentials` and `seed_claude_local_install`, which duplicate the recipes versions.
- Builds the context dir with `tddy_core::backend::context_globs_for_agent(kind)`.
- Ports: gRPC listen and shim.
- Runner argv: `--session-id --context-dir --cwd --grpc-socket --tool-ipc-socket --tddy-tools-path --ready-marker --model --grpc-listen-port --egress-shim-port --initial-cols --initial-rows`, then `--claude-binary --permission-mode --claude-arg…` for Claude or `--agent-kind cursor --agent-binary --agent-arg…` for Cursor, plus `--mcp-log-level`.
- Env = `build_sandbox_runner_env` + `subagent_env_overlay` (`TDDY_SUBAGENT`, `TDDY_SUBAGENTS_JSON`, `TDDY_SUBAGENT_ROSTER_STATIC=1`, `TDDY_SUBAGENT_REPLACES`) + `TDDY_LSP_TOOLS` + `SESSION_ACTION_TOOLS_ENV=1`.
- Mounts from `build_sandbox_mounts(mode, repo, home)`: repo rw unless Managed, plus home rw.
- `host_home: $HOME`, so the per-session credential copy stays on.
- Uses a default cgroup and `route: None`.

**`src/host_agent.rs`** (sandboxed-codebase mode, agent on the host): `host_mcp_env` sets `TDDY_SANDBOX_TOOL_IPC=<socket>`, the subagent overlay, and `TDDY_TOOLS_LOG_FILE` / `TDDY_TOOLS_ACCOUNTING_FILE`. `TDDY_SANDBOX_TOOL_IPC` "is the whole mechanism" for tool-transport selection.

#### tddy-tools / tddy-session-tool-client (how the in-jail MCP knows it is jailed)

- `tddy-session-tool-client/src/lib.rs:248`: `detect_session_tool_transport()` returns `SandboxIpc{socket_path}` whenever `TDDY_SANDBOX_TOOL_IPC` is set. Otherwise it falls back to the LiveKit or HTTP env.
- `tddy-tools/src/server.rs:1748`: dynamic tool dispatch goes over sandbox IPC when that var is set. The server advertises exec tools only when a session-tool transport exists. A todo notes the transport selection exists in three copies.

#### tddy-spawn / tddy-pty

- **tddy-spawn** is explicitly not sandbox ("share no `crate::` edge … Confinement and privileged fork are different concerns").
  - It covers spawn-as-another-user (`StagedCommand`, `privilege_drop`), clones, and the spawn worker or supervisor (`SpawnBackendChoice::{Supervisor{socket}, ForkedWorker}`).
  - The supervisor header says the daemon "asks `tddy-supervisor` to spawn sessions, clone repos and build sandbox jails".
  - `spawner.rs` writes the agent def to a file (`write_agent_def_file`) rather than argv or env.
- **tddy-pty** has `PtySpawnSpec { argv, worktree_path, session_id, terminal_id, kind, env: Vec<(String,String)> }`, used for unsandboxed PTYs. "Per-user PATH/HOME live in the daemon." The in-jail PTY uses `portable_pty` directly in the runner, not tddy-pty; only the resize decoder is shared.

#### 3. Grep / glob log

| Pattern | Scope | Notable hits |
|---|---|---|
| `sandbox\|spawn\|pty\|agent-launch\|daemon` | `ls packages` | 9 sandbox crates plus tddy-spawn, tddy-pty, tddy-agent-launch |
| `pub fn\|claude\|TDDY_` | `tddy-sandbox/src/context_dir.rs` | `MANAGED_CODEBASE_PREAMBLE` (19), `PREAMBLE_FILES` (43), `SandboxContextDir` (198) |
| `fn \|allow\|deny\|network\|claude\|oauth` | `darwin/src/profile.rs` | `render_plan` (25), `context_dir_from_runner_argv` (193), `render_network` (226), oauth (235) |
| `^pub fn\|^pub const` | `recipes/src/{claude,cursor}_cli.rs` | `PERMISSION_PROMPT_TOOL` (164), `claude_runner_env_overlay` (154), `cursor_runner_env_overlay` (315) |
| `env(\|HTTPS_PROXY\|TDDY_\|--model\|mcp_env` | `runner/src/runner.rs` | `HTTPS_PROXY` set (1285–1288), `resolve_secret_envs` (1067), `TDDY_SUBAGENT` (1563), `TDDY_SEMANTIC_INDEX_DB` (1576), `egress_proxy_env` (1793), `start_egress_shim` (1809) |
| `fn build_claude_base_argv` | packages | `tddy-agent-backend/src/claude_argv.rs:15` |
| `pub struct ActionSpec\|SandboxRequest\|ActionInput` | `tddy-actions/src` | `spec.rs:25,51,93` |
| `pub fn\|claude\|TDDY_\|HOME\|PATH` | `daemon-sandbox/src/sandbox_session.rs` | `build_sandbox_runner_env` (495), `SandboxRunnerSpawn` (762), `build_sandbox_plan` (790), `prepare_persistent_claude_home` (837), `qemu_backend_requested` (894) |
| `resolve_claude_home_dir\|claude_home_dir` | `tddy-daemon-kernel/src` | `config/agent_cli.rs:168,414`; `CLAUDE_HOME_ENV` = `TDDY_SANDBOX_CLAUDE_HOME` |
| `runner_argv\|"--\|mounts\|host_home\|env.extend` | `agent-launch` cursor and harness starts | cursor 274 (`prepare_persistent_cursor_home`), 287–312 (argv), 367 (mounts); harness 200 (per-session home), 275 (mounts) |
| `pub fn\|recipe\|"--\|MountSpec` | `workspace_tool_sandbox.rs` | `build_workspace_tool_plan` (158), recipe Shell (229), `host_ripgrep_grants` (291) |
| `HostRelayConfig\|TunnelOpen\|TcpStream::connect` | `host_relay.rs` | 246, 773, 889 (unfiltered connect) |
| `pub fn\|memory_max\|DEFAULT` | `cgroups/src/lib.rs` | `spawn_plan` (79), `default_limits` (218), `enter_rootless_jail` (621) |
| `sandbox` | `tddy-spawn/src`, `tddy-pty/src` | only comments (`lib.rs:5`, `supervisor_client.rs:4`, `resize.rs:5`) |
| `TDDY_SANDBOX_TOOL_IPC\|SESSION_ID\|EGRESS_DIR\|BACKEND` | packages (non-test) | `tddy-tools/src/server.rs:1748,3432`; `tddy-session-tool-client/src/lib.rs:248`; `sandbox-app/src/host_agent.rs:71,88`; `daemon-sandbox sandbox_session.rs:895` |
| `-li sandbox` | `docs/dev/todo/*` | 83 files (listed in §4) |
| `-iname '*sandbox*'` | `docs/ft` | `coder/sandbox-builder.md`, `coder/sandboxed-codebase-mode.md`, 2 daemon amendments, 8 changelogs |
| `ToolSpec\|tools/*.yaml\|<tddyhome>/tools` | packages, docs | only `workspace_tool_sandbox.rs` and `todo/2026-09-26-grep-is-unreachable-inside-every-jail.md` |
| `REMOTE_APPENDIX` | packages | `tddy-coder/src/remote.rs:38` (the non-sandbox remote mode's prompt appendix) |
| `pub struct HarnessRoute\|CliHarness` | packages | `tddy-agent-backend/src/{provider_route.rs:155, cli_harness.rs:51}` |

#### 4. Findings

#### 4.1 Architecture: config → plan → backend → runner → exec

1. **Caller.** The caller assembles inputs by hand: a coding session (`tddy-agent-launch` `svc_start_sandboxed_{claude,cursor,cli_harness}_session`, relaunch), the standalone app (`tddy-sandbox-app spawn.rs`), actions (`daemon-sandbox sandbox_plan_builder`), or workspace jails (`workspace_tool_sandbox`). The inputs are:
   - **directories**: `<session>/sandbox` (project_root), `.work` (scratch), `.work/tmp`, `context/`, `<session>/egress`;
   - **binaries**: `tddy-sandbox-runner`, `tddy-tools`, and the agent binary resolved to an absolute canonical path;
   - **runner argv**: a CLI flag soup. This is the only way the runner learns what to exec;
   - **env**: `scratch_runner_env` + an agent overlay + feature env (`TDDY_SUBAGENT*`, `TDDY_LSP_TOOLS`, `TDDY_RESTRUCTURE_TOOLS`, `TDDY_SEMANTIC_INDEX_DB`, `TDDY_DAEMON_INSTANCE_ID`, `CLAUDE_FULLSCREEN_ENV`);
   - **mounts** (the persistent home, rw; the repo in mounted mode);
   - `host_home`, `route`, `cgroup`, and the loopback ports (the shim, and gRPC for the app).
2. **Plan assembly.** `tddy_daemon_sandbox::sandbox_session::build_sandbox_plan(SandboxRunnerSpawn)` calls `tddy_sandbox_recipes::build_runner_plan(RunnerPlanRequest)`. The recipe is **inferred from runner argv** (`--agent-kind` / `--claude-binary` / `--pty-command`).
   - The recipe supplies reads (baseline + toolchain + `otool` + agent install tree), policy, credential copies, the `CLAUDE_CODE_OAUTH_TOKEN` secret (from the daemon's own env, Claude only, unrouted), `allow_oauth_inbound`, and route env, secrets and files.
   - On macOS, `merge_allow_read_paths_into_plan` adds detected toolchain and binary directories as exec subpaths.
   - The output is `SandboxPlan`.
3. **Backend.** `spawn_sandbox_plan` dispatches on `TDDY_SANDBOX_BACKEND=qemu` or the OS:
   - darwin: render SBPL, run `sandbox-exec -f profile /usr/bin/env -i K=V… <runner argv>`;
   - cgroups: userns+netns+mountns with RO bind mounts, `env_clear` + plan env, a cgroup scope with limits;
   - qemu: 9p mounts + `plan.json`.
   - All three materialise copies, symlinks and secrets first. Both darwin and cgroups detect `--stdio` in argv and pipe stdio.
4. **In-jail runner.** `tddy-sandbox-runner` starts the egress shim (a loopback CONNECT proxy) and the tool-IPC UDS server, then a PTY whose argv is built *inside the runner* from flags:
   - Claude: `build_claude_base_argv` + `--claude-arg` + MCP allow/disallow + `--permission-prompt-tool` + `--mcp-config`, with the MCP config file written to `$TMPDIR`;
   - Cursor: `node index.js` + `.cursor/mcp.json` in `$HOME`;
   - opencode and muse: `cli_harness` descriptors;
   - generic `--pty-command`.
   - The agent child inherits the runner env, plus `HTTPS_PROXY`/`HTTP_PROXY` (both casings) set to the shim, `TDDY_EGRESS_SHIM`, and the `TDDY_SECRET_*` values resolved to their real names (files unlinked).
   - Then the runner serves `SandboxService` (`SessionChannel`) over stdio (daemon) or loopback gRPC / UDS (app, actions).
5. **Host relay.** `run_host_relay*`:
   - answers `HostPoll` tool calls through a `HostToolHandler`; the daemon's handler runs `tool_engine` in the worktree or conversation worktree with `session_env`;
   - forwards PTY output and sizes;
   - dials real TCP for each `TunnelOpen` (TLS stays end-to-end);
   - forwards allowlisted RPCs (`IN_JAIL_RELAYABLE`) to `HostRpcHandler`.
6. **In-jail tddy-tools.** It sees `TDDY_SANDBOX_TOOL_IPC` and dispatches every exec tool over the UDS to the runner, which queues it for the host.

#### 4.2 How each concern is declared today

| Concern | Where | Generic or agent-specific |
|---|---|---|
| Writable tree | `SandboxSpec{project_root, scratch_dir, egress_dir}` + `MountSpec{writable}` | generic |
| Read and exec allow-list | `ReadSpec`s from recipes (`system_baseline_reads`, `process_exec_reads`, cursor install tree, `otool -L`), daemon `build_allow_read_paths` (argv sniffing), workspace ripgrep FIXME | mechanism generic; contents per recipe and agent |
| Context dir (read-only inside the jail) | `--context-dir` argv; darwin profile **parses runner argv** to emit the write-deny | agent concept wired through argv sniffing |
| HOME | `scratch_runner_env` sets `HOME=scratch_home`. Callers pass either the daemon-wide persistent `~/.tddy/sandbox-claude-home` / `sandbox-cursor-home` (mounted rw) or the per-session `.work/home` (opencode, muse, actions, workspace) | mechanism generic; home choice and seeding agent-specific |
| TMPDIR | `.work/tmp`; plus `CLAUDE_CODE_TMPDIR`/`CLAUDE_TMPDIR` or `CURSOR_TMPDIR` overlays | overlay agent-specific |
| PATH | hardcoded `/usr/bin:/bin:/usr/sbin:/sbin` (`runner_env.rs:12`); only the workspace jail prepends the `rg` directory. Binaries are exec'd by absolute path from argv, not via PATH | no declarative "binaries on PATH" |
| Exec permission | `PolicySpec.exec_paths` per recipe (Claude: `/Applications`, `/opt/homebrew`, `/opt/local` …) + exec reads + all mounts + project_root | per recipe |
| Env passthrough | none from the host except `RUST_LOG`, `TDDY_EGRESS_PROBE_*` and (as a secret) `CLAUDE_CODE_OAUTH_TOKEN`. Everything else is caller-built | |
| Credentials | Claude: `.claude/.credentials.json` (seed once into the persistent home, or per-session `CopySpec` when `host_home=Some`) + `CLAUDE_CODE_OAUTH_TOKEN` secret. Cursor: `~/.cursor/{auth,cli-config,mcp}.json`. Routes: `HarnessRoute.secret_env` becomes `TDDY_SECRET_*`; `route.files` go to `scratch/provider-route/` and the config env (`OPENCODE_CONFIG`) | agent-specific; the secret channel itself is generic |
| Network egress | darwin `(deny network*)` + loopback ports + UDS; Linux netns with `lo` only. Outbound only via the runner's CONNECT shim, and the host relay dials **any** host:port with no allowlist. `allow_oauth_inbound` (Claude/Cursor unrouted) opens `localhost:*` inbound | no per-tool egress declaration; no domain policy |
| PTY / stdio | runner flags (`--pty-command`, `--agent-kind`, `--stdio`, `--initial-cols/rows`); `PolicySpec.pseudo_tty`; darwin static `file-ioctl` for PTY devices; backends sniff `--stdio` | mostly generic, but PTY mode is chosen by agent kind |
| Resources | `ResourceLimits` (cgroups defaults 2 GiB / 1 CPU / 512 pids); `CgroupConfig` from daemon `sandbox_cgroup:` | generic |
| What to exec | runner argv flags; the runner builds the agent argv itself (`build_claude_base_argv`, cursor node/index.js, `cli_harness().argv`) | the runner is agent-aware |
| Tool surface for the agent | MCP config pointing at `tddy-tools --mcp` with env `TDDY_TOOLS_LOG_FILE`/`ACCOUNTING_FILE`/`RUST_LOG`; `--allowedTools`/`--disallowedTools`/`--permission-prompt-tool` (Claude only); tool-IPC UDS (`TDDY_SANDBOX_TOOL_IPC`) | Claude-specific; Cursor is guidance-only |

#### 4.3 Where the agent is told it is sandboxed

The child receives:
- **env**: `TDDY_SANDBOX_SESSION_ID`, `TDDY_SANDBOX_TOOL_IPC`, `TDDY_SANDBOX_EGRESS_DIR`, `TDDY_EGRESS_SHIM`, `HTTPS_PROXY`/`HTTP_PROXY`/`https_proxy`/`http_proxy`, `TDDY_SECRET_<N>` (on the runner; resolved values on the child), `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON`/`TDDY_SUBAGENT_REPLACES`/`TDDY_SUBAGENT_ROSTER_STATIC`, `TDDY_SEMANTIC_INDEX_DB`, `TDDY_LSP_TOOLS`, `TDDY_RESTRUCTURE_TOOLS`, `TDDY_DAEMON_INSTANCE_ID`, and the session-action env (app only);
- **prompt or files**: `MANAGED_CODEBASE_PREAMBLE`, prepended to `CLAUDE.md` and `AGENTS.md` in the context dir, plus subagent and Shell-replacement paragraphs. The managed-workflow `--append-system-prompt-file` lives in the context dir;
- **flags**: `--allowedTools` (only `mcp__tddy-tools__*` + `AskUserQuestion` + subagent tools), `--disallowedTools` (replaced tools and native aliases), `--permission-prompt-tool mcp__tddy-tools__approval_prompt`, `--mcp-config`.

In-jail `tddy-tools` switches transport purely on `TDDY_SANDBOX_TOOL_IPC`.

#### 4.4 Generic vs agent-specific: the split today and where it leaks

**Genuinely generic:**
- the `SandboxBuilder` / `SandboxPlan` types (except `allow_oauth_inbound`);
- `materialize_*`;
- the SBPL and bind-mount renderers (apart from the Claude header comment and the `--context-dir` / `--stdio` argv sniffing);
- cgroups and qemu spawn;
- `system_baseline_reads`, `binary_exec_reads`, `path_traversal_reads`;
- `process_jail_env`;
- the secret file channel;
- the host relay CONNECT tunnel;
- the generic `--pty-command` runner mode.

**Agent-specific logic spread outside a single "tool spec":**
1. **Recipes crate.** `SandboxRecipe` enum arms per harness; Claude, Cursor, Muse and OpenCode policies, copies and env overlays; Claude MCP/allowlist argv; Cursor argv rewriting.
2. **Runner.** `--claude-binary`, `--agent-kind`, `--claude-arg`, `--agent-arg`, `--permission-mode`, `--dangerously-skip-permissions`, `--resume`, `--append-system-prompt-file`, `--mcp-log-level`. `spawn_claude_pty` and `spawn_cursor_pty` hardcode MCP config placement (Claude: `$TMPDIR/claude-mcp-config.json`; Cursor: `$HOME/.cursor/mcp.json`), and the runner reads `TDDY_SUBAGENT*` and `TDDY_SEMANTIC_INDEX_DB` to shape agent argv. `HTTPS_PROXY` is applied only on the agent paths (`run_claude_pty_thread`), not on `--pty-command`.
3. **Recipe inference by argv sniffing.** `detect_recipe_from_argv`, `agent_binary_from_runner_argv`, daemon `build_allow_read_paths` (`--claude-binary`, cursor `--agent-binary`), darwin `context_dir_from_runner_argv`, darwin and cgroups `--stdio`, cgroups `--grpc-uds` / `--ready-marker`.
4. **Generic crate pieces that are tddy-tools / Claude knowledge.**
   - `tddy-sandbox::workspace_exec_tool_names()`, `MANAGED_CODEBASE_PREAMBLE`, `PREAMBLE_FILES = [CLAUDE.md, AGENTS.md]`;
   - `NetworkSpec.allow_oauth_inbound`;
   - the profile header "for sandboxed Claude Code CLI sessions";
   - docs that say the runner sets secrets "on the inner claude child".
5. **Persistent `.claude` home.**
   - `tddy-daemon-kernel::resolve_claude_home_dir` (`TDDY_SANDBOX_CLAUDE_HOME` / `claude_cli.claude_home_dir` / `~/.tddy/sandbox-claude-home`) feeds `prepare_persistent_claude_home`, which runs `seed_claude_credentials` (copy-once) and `seed_claude_local_install` (`.local/bin/claude` and `versions` symlink mirror). The result is mounted rw with `host_home: None`.
   - Cursor has a parallel `sandbox-cursor-home`.
   - OpenCode and Muse use a per-session `.work/home` and require a provider route.
   - The app has duplicate `seed_claude_credentials` / `seed_claude_local_install` in `tddy-sandbox-app/src/spawn.rs` and passes `host_home=$HOME`, so it also copies per session.
6. **Overlay misapplied.** `build_sandbox_runner_env` always adds `claude_runner_env_overlay`, so opencode, muse, cursor and app jails get `CLAUDE_CODE_TMPDIR`.
7. **Daemon OAuth token.** `CLAUDE_CODE_OAUTH_TOKEN` comes from the **daemon process env** in `finish_builder` (a hidden host passthrough). `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN` and `ANTHROPIC_*MODEL` arrive only via `HarnessRoute` (provider accounts).
8. **Three near-copies of the launch sequence** (Claude start, Cursor start, relaunch, plus the harness start), each hand-building argv, env and mounts. See the todo `2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md` (76% line-for-line overlap).
9. **Closest existing declarative pieces.**
   - `tddy-actions::ActionSpec` + `SandboxRequest{output_dir, extra_read_paths, recipe: Option<String>, stdin}` + `ActionInput{host_path, jail_path, writable}` (serde/YAML, used for actions only, with the recipe chosen by name);
   - `SandboxAppConfig` (YAML, but agent-centric: `claude_binary`, `claude_home_dir`, `claude_args` …);
   - `CliHarness` static descriptors (argv fn, binary env, `route_config_env`, `context_agent`);
   - the planned-but-unbuilt `ToolSpec[]` / `<tddyhome>/tools/*.yaml`, described in the `workspace_tool_sandbox.rs` comment and the grep todo: "names a host binary or a `tddy-build` target, each stating what becomes readable, executable and reachable on `PATH`; a tool the host cannot satisfy refuses at jail startup".
10. **Egress policy gaps.** No declared egress allowlist: the host relay `spawn_tunnel` connects to any `host:port`. Egress is enabled per jail by the presence of `--egress-shim-port`, not by a policy object. Only the workspace jail omits the shim.

#### 4.5 Docs

**Package docs:**
- `packages/tddy-sandbox/docs/architecture.md` (main reference);
- `packages/tddy-sandbox/docs/code-issues/oversized-file-builder.md`;
- `packages/tddy-sandbox-darwin/docs/troubleshooting.md` (profile reproduction, blockers, the HTTPS_PROXY CONNECT tunnel, macOS logs);
- `packages/tddy-sandbox-qemu/docs/guest-image-9p-init.md`;
- `packages/tddy-daemon-sandbox/docs/task-and-action-services.md`;
- `changesets/` in every crate, of note: the runner's `2026-10-22-provider-route-sandboxed-routes.md` and `2026-09-09-unbundle-session-agent-services.md`; the recipes' `2026-07-04-seed-claude-credentials-seed-claude-local-install.md` and `2026-07-27-cursor-runner-env-overlay-skips-macos-keychain.md`; the app's `2026-07-04-explicit-host-home-on-sandboxrunnerspawn-behavior-preserving.md`;
- `code-issues/` in the runner, app and daemon-sandbox (oversized `runner.rs` / `host_relay.rs`, heavy params in `dial_and_bridge`).

**Feature docs:**
- `docs/ft/coder/sandbox-builder.md` (builder PRD: explicit plan, strict reads, the Claude recipe, action plans);
- `docs/ft/coder/sandboxed-codebase-mode.md`;
- `docs/ft/daemon/claude-cli-session.md` § Sandbox mode (lines 67–103) and § Running on a provider account (`ANTHROPIC_*` via route, secrets via `TDDY_SECRET_*`, no OAuth inbound when routed);
- `docs/ft/daemon/remote-codebase-mode.md` (§ Workspace tool sandbox);
- `docs/ft/daemon/agent-context-sync.md`;
- `docs/ft/daemon/{cursor,opencode,muse,warp}-cli-session.md`, `docs/ft/daemon/provider-accounts.md`, `docs/ft/daemon/session-agent-roster.md`;
- `docs/ft/daemon/amendments/PRD-2026-08-31-split-sandbox-orchestration.md`, `PRD-2026-09-20-sandboxed-codebase-managed-workflow.md`;
- changelogs: `docs/ft/daemon/changelog/2026-06-27-darwin-sandboxed-claude-cli-sessions-local-grpc.md`, `2026-06-28-linux-cgroups…`, `2026-06-29-unified-actions-tasks-with-optional-sandbox-execution.md`, `2026-07-06-cursor-cli-sandbox-parity…`, `2026-07-11-unprivileged-daemon-linux-cgroups…`, `2026-09-19-sandboxed-codebase-placement.md`, `2026-10-22-provider-route-sandboxed-routes.md`; `docs/ft/coder/changelog/2026-07-03-claude-sandbox-launcher-yaml-config.md`, `2026-07-21-agent-driven-tool-replacement-no-bash-and-no-write-sandbox.md`, `2026-09-05-sandboxed-codebase-mode.md`;
- also mention sandbox: `docs/ft/coder/{no-bash-mode, managed-codebase-subagents, specialized-subagents, session-actions}.md`, `docs/ft/vm/tddy-vm.md`, `docs/ft/supervisor/tddy-supervisor.md`.

#### 4.6 `docs/dev/todo` entries mentioning sandbox (83 of 400)

Format: filename [number of "sandbox" mentions] — gist.

**Most design-relevant:**
- `2026-09-26-grep-is-unreachable-inside-every-jail.md` [5] — the jail PATH lacks Homebrew `rg`, so `Grep` fails in every Mac jail. It was patched by `host_ripgrep_grants`, and the designed fix is **`ToolSpec[]`**: a declared external-tool list stating reads, exec and PATH additions, refusing at startup when unsatisfiable.
- `2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md` [20] — the Claude, Cursor and relaunch jail launches are three copies (76% overlap); a shared `svc_sandboxed_jail_launch` is staged but not merged.
- `2026-10-24-warp-cli-sandbox.md` [4] — warp-cli is refused in the sandbox; it would need a runner agent kind, a Warp config-dir allow-list and a recorder.
- `2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md` [9] — the app has no provider route, vault, binary flags or persistent home for these kinds; the recommendation is to resolve the route via the daemon.
- `2026-10-24-sandboxed-main-needs-session-role.md` [12] — the sandboxed claude-cli start drops `session_role`, so a project-home main cannot be sandboxed.
- `2026-10-18-lsp-tools-in-a-sandbox-jail-never-reach-the-host-index.md` [6] — in-jail `Lsp*` and `restructure_*` don't reach the host index; this needs a forwarding executor over the bridge.
- `2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md` [2] — cursor-agent has no allow/disallow tools, so it can only be guided.
- `2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md` [21] — shutdown doesn't stop jailed runners, and nothing watches for runner death.
- `2026-08-17-session-agent-roster-the-sandbox-bridge-and-other-deliberate-gaps.md` [9] — roster gaps at the sandbox bridge.
- `2026-08-23-the-action-tools-are-advertised-where-nothing-implements-them.md` [3] — session action tools are advertised without a handler.
- `2026-09-05-from-2026-09-05-sandboxed-codebase-mode.md` [7] — follow-ups from sandboxed-codebase mode.
- `2026-07-03-tddy-sandbox-app-tddy-sandbox-runner.md` [4] — no CI test of a full `./claude-sandbox` launch with an inline Ollama def; the launcher docs may need splitting out.
- `2026-10-01-the-sandbox-runner-links-git-code-through-a-types-re-export.md` [3] — the runner links `tddy-subagent-worktree` through a re-export.
- `2026-10-01-session-tool-client-repeats-its-transport-selection-three-times.md` [1] — the `TDDY_SANDBOX_TOOL_IPC` and other transport selection is duplicated.
- `2026-09-26-a-jail-rebuild-can-re-run-a-tool-call-that-already-executed.md` [1] — workspace jail rebuild can re-run a call that already executed.
- `2026-10-04-session-restructure-tools-no-jail-end-to-end-test.md` [2] — no test of restructure tools reaching the host from a jail.
- `2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md` [2] — start phases skip the sandboxed, tool and split starts.

**Linux, cgroups, qemu, supervisor:**
- `2026-06-28-tddy-sandbox-cgroups.md` [3] and `2026-07-02-tddy-sandbox-cgroups.md` [2] — cgroups follow-ups.
- `2026-08-02-unprivileged-userns-available-under-approximates-what-the-jail-needs.md` [4] — the userns probe is too weak.
- `2026-08-02-tddy-supervisor-follow-ups.md` [2] and `2026-08-03-tddy-supervisor-deliberate-gaps-and-follow-ups.md` [5] — the supervisor builds jails.
- `2026-08-14-vm-image-chaining-and-testkit-deliberate-gaps.md` [1] and `2026-08-30-a-guest-side-rpc-driver-for-vm-backed-daemon-acceptance.md` [2] — VM backend.
- `2026-10-08-keyring-supervisor-host-socket-real-run.md` [1] — supervisor host socket never run under a real root supervisor.

**Tests and CI hygiene:**
- `2026-08-01-cargo-test-workspace-has-7-pre-existing-failures-on-linux-not-3.md` [11]
- `2026-08-15-echoes-a-message-over-sandbox-service-served-over-stdio-is-skipped-in.md` [5] — the stdio echo test is skipped in CI.
- `2026-09-09-daemon-sandbox-suites-never-call-set-self-handle.md` [6] — 17 tests panic in `self_arc`.
- `2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md` [8]
- `2026-09-19-action-sandbox-acceptance-pty-test-does-not-finish.md` [9]
- `2026-09-16-three-comments-claim-release-does-not-build-the-sandbox-runner.md` [7]
- `2026-09-16-the-shipped-binary-set-is-restated-in-eight-places-and-one-is-pinned.md` [2]
- `2026-07-26-tddy-sandbox-recipes-host-independent-sandbox-path-tests.md` [2]
- `2026-09-09-the-test-script-reports-green-on-a-red-suite.md` [1], `2026-08-02-test-hides-every-target-after-the-first-failure.md` [1]
- `2026-08-13-deterministic-test-suite-deliberate-gaps.md` [2]
- `2026-09-26-five-absence-assertions-in-the-new-subagent-suites-have-no-positive-control.md` [1]
- `2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md` [2]

**File budgets and refactors:**
- `2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md` [5] — `tddy-spawn` and `tddy-daemon-sandbox` were moved unsplit.
- `2026-09-24-lifecycle-files-over-the-400-line-target.md` [5], `2026-09-24-lifecycle-functions-still-over-150-lines.md` [9], `2026-09-24-lifecycle-modules-to-re-parent-by-hand.md` [3]
- `2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md` [1]
- `2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md` [4]
- `2026-10-01-files-the-agent-worktree-change-grew-past-the-budget.md` [4], `2026-10-03-caller-sync-grew-over-budget-files.md` [1]
- `2026-09-24-keyring-desktop-login-grew-thirteen-over-budget-files.md` [4]
- `2026-10-08-narrow-the-items-the-carve-moves-widened-to-pub-that-no-other-crate-uses.md` [2]
- `2026-10-08-drain-tddy-discovery-into-purpose-named-crates-and-remove-it.md` [3]
- `2026-10-09-restructure-extract-method-rust-analyzer-rewrites-that-do-not-build.md` [1], `2026-10-09-restructure-move-to-crate-names-a-re-exporting-crate-instead-of-the-defining-one.md` [3]
- `2026-09-10-three-crates-gained-a-transitive-tddy-tui-dependency.md` [1]
- `2026-09-23-atomic-file-leaf-crate.md` [1] — two copies of the atomic secret write.
- `2026-09-09-tddy-daemon-untested-complexity-hotspots.md` [2]

**Context sync** (the jail's context dir):
- `2026-08-29-agent-context-sync-the-re-sync-trigger-is-not-wired.md` [1]
- `2026-08-29-agent-context-sync-tddy-coder-src-remote-rs-holds-a-contradictory-dead.md` [1]
- `2026-10-28-context-sync-link-pulled-in-trees-are-unbounded.md` [1]
- `2026-10-28-context-sync-git-gate-fails-a-whole-stage-on-one-submodule-path.md` [1]
- `2026-10-28-context-sync-git-gate-misses-private-worktree-exclude.md` [1]
- `2026-10-28-context-reader-resolve-then-read-race.md` [2]
- `2026-10-28-context-write-guard-refuses-case-folded-directories-on-macos.md` [1]

**Other, with sandbox mentioned in passing:**
- `2026-06-16-tddy-build.md`, `2026-07-01-tddy-core.md`, `2026-07-01-tddy-daemon.md`, `2026-07-02-tddy-coder.md`, `2026-07-02-tddy-sandbox-app.md` [2], `2026-07-22-tddy-core-tddy-coder-tddy-daemon.md`
- `2026-07-26-pr-stack-status-polling-and-stack-hygiene.md`, `2026-07-28-terminal-native-scrolling-model.md`, `2026-07-28-terminal-lazy-scroll-up-livekit-transport-unified-surface.md`
- `2026-08-13-pr-stack-a-managed-claude-cli-cursor-cli-session-may-record-a-goal-id.md` [2], `2026-08-15-session-worktree-sync-deliberate-gaps.md` [3], `2026-08-16-models-agents-open-items-at-wrap.md`, `2026-08-30-pr-stack-querybranch-s-local-only-legs.md` [5]
- `2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md`, `2026-09-15-rpcservice-has-no-cancellation-surface.md` [2], `2026-09-20-tauri-dev-restarts-the-app-for-generated-test-fixtures.md` [2], `2026-09-20-remove-the-specialized-agents-tombstone-from-sessionmetadata.md`
- `2026-09-27-desktop-mints-a-new-daemon-identity-per-restart-and-orphans-its-sessions.md` [2], `2026-10-03-a-subagent-made-merge-is-read-as-a-caller-sync.md`
- `2026-10-17-desktop-cursor-hooks-reach-daemon.md` [6], `2026-10-22-tool-session-starts-ignore-the-provider-default.md` [1]

#### 4.7 Implications for the redesign

These are raw observations for the parent doc.
- No single object today describes a runnable tool's sandbox needs. That knowledge is spread across:
  - the recipe enum arms;
  - runner flags and runner-internal argv building;
  - daemon/app launch functions that hand-build env, mounts and homes;
  - argv sniffing in three layers (recipes, daemon read paths, darwin profile);
  - agent-flavoured bits in the generic crate (`allow_oauth_inbound`, `workspace_exec_tool_names`, `MANAGED_CODEBASE_PREAMBLE`).
- The runner is not tool-agnostic. The only tool-agnostic exec path is `--pty-command`, and it lacks the proxy env and secret injection the agent paths get.
- Candidate fields a declarative tool YAML would need to replace current code:
  - binary resolution (`binary_env`, `default_binary`, PATH search);
  - reads and exec (`process_exec_reads` / install tree / `otool`);
  - policy (JIT, fork, mach, sysctl, pty);
  - home strategy (persistent daemon-wide vs per-session; seed-once files; install-layout symlinks);
  - tmp env overlays;
  - credential copies and secrets (OAuth token, route secrets and files, config env var);
  - MCP registration placement (`--mcp-config` file vs `$HOME/.cursor/mcp.json`) and the tool allow/deny argv;
  - context globs and preamble files;
  - egress (shim on or off, plus a host allowlist that doesn't exist yet), OAuth inbound;
  - PTY vs stdio, resume and session-id argv shape.
- The prior art to reconcile with: `ActionSpec` / `SandboxRequest`, `CliHarness` descriptors, `SandboxAppConfig`, and the designed `ToolSpec[]` / `<tddyhome>/tools/*.yaml`.

### Pass 3 — inventory of coding-agent-specific knowledge

#### Discovery dump: where coding-agent-specific knowledge lives today

Repo: `/Users/mantasi/Code/tddy-coder/.worktrees/redesign-coding-agents` (HEAD `f48a5e9b4`). The work was read-only. All paths below are relative to `packages/` unless they start with `/` or `docs/`.

#### 1. Sequence (chronological)

1. `ls packages/` and `git log -3`: crate inventory and baseline.
2. `find` over the 11 candidate crates (`*.rs|toml|yaml|json|md`): file inventory.
3. `grep -rlE 'cursor-agent|codex-acp|"codex"|opencode|"claude"|claude-agent-acp' --include=*.rs`, counted per crate. This ranked the hotspots: session-lifecycle 25 files, agent-backend 25, agent-launch 24, daemon-rpc 16, integration-tests 13, coding-subagent 8, sandbox-recipes 7, and others.
4. `wc -l` on agent-backend plus `grep 'pub (trait|enum)'`: found the traits and enums.
5. Read `tddy-agent-backend/src/backend/mod.rs` (AnyBackend, CodingBackend, InvokeRequest, model catalogs, selection question).
6. Read `tddy-agent-backend/src/provider_route.rs` (Harness enum, route matrix).
7. Grepped then read `backend/claude.rs` (argv, MCP temp file, env, stream).
8. Read `claude_argv.rs`, `spawn_env.rs`, `cli_harness.rs`, `cli_harness_muse.rs`, `cli_harness_warp.rs`, `lib.rs`.
9. Read `claude_hooks.rs` and `cursor_hooks.rs`.
10. Grepped then read `backend/cursor.rs`, `codex.rs`, `codex_acp.rs`, `opencode_acp.rs`, `acp.rs`, `routed.rs`, `provider_route/codex.rs`.
11. Read `backend/headless_driver.rs` and `headless_driver/argv.rs`.
12. Read `backend/model_catalog.rs`, `context_globs.rs`, `stream/mod.rs`, `provider_route_file.rs`; grepped `claude/transcript_usage.rs`, `acp_driver*`, `codex_acp/oauth.rs`, `codex/login_text.rs`.
13. Grepped `tddy-coder` and `tddy-coder-cli` for agent selection; read `run.rs` (`verify_tddy_tools_available`, `create_backend`, `BUILTIN_BACKEND_AGENT_IDS`, Args) and `codex_binaries.rs`.
14. Grepped `TDDY_CODEX_OAUTH_OUT` (found `tddy-coder-cli/src/main.rs`) and `tddy-coder/src/remote.rs`.
15. Read all of `tddy-sandbox-recipes`: `lib.rs`, `claude_cli.rs` (first 430 lines), `cursor_cli.rs`, `opencode_cli.rs`, `muse_cli.rs`, `route_files.rs`, `plan.rs`.
16. `tddy-agent-launch`: `wc` plus per-file agent-mention counts, then `lib.rs`, `claude_cli_spawn.rs`, and a large grep for env vars, flags and dotfiles.
17. Read `svc_start_session_core.rs` (session_type dispatch), `cursor_cli_spawn.rs` (rules dir), `chat.rs` (cursor hooks), `claude_cli_spawn_steps.rs`.
18. Read `tddy-daemon-kernel/src/config/agent_cli.rs`, `config/access.rs` and `config.rs` (daemon.yaml schema).
19. Counted agent mentions in the sandbox crates. Read `tddy-sandbox-runner/src/runner.rs` (Args and agent_kind dispatch) and grepped `harness_pty.rs`.
20. `tddy-sandbox-app`: grep, then `spawn.rs` (AgentKind) and `host_agent.rs` (`build_host_agent_argv`).
21. `tddy-daemon-sandbox` and `tddy-sandbox`: grep, then `sandbox_session.rs` (runner env, persistent homes), `action_service.rs`, and the `context_dir.rs` preamble.
22. Grepped actions, tool-engine, toolcall, workflow, workflow-engine, workflow-recipes and core. Read `tddy-actions/src/catalog.rs`, `tddy-workflow/src/hints.rs` and `tddy-workflow-recipes/src/permissions.rs`.
23. Grepped session-lifecycle, session-split and cli-sessions. Read `tddy-session-lifecycle/src/coding_agent_launcher.rs` (lines 1–900).
24. Read `tddy-coding-subagent`: `confinement.rs`, `catalog.rs`, `transports.rs`, and the `launcher.rs` signatures.
25. Read `tddy-discovery/src/harness_profile.rs` and grepped the `agent_def.rs` fields.
26. Big grep of agent literals across about 25 remaining crates.
27. Read `tddy-session-files/src/context_files.rs`, `tddy-daemon-rpc/src/catalog/agent_models.rs` and `catalog/ports.rs`, the `tddy-session-activity` hook_reports gate and `tddy-tools/src/session_hook.rs`.
28. Read `tddy-changeset/src/session_activity.rs` (hook event to status mapping).
29. Read `tddy-cli-sessions/src/cli_session_manager/argv.rs` and `launch.rs`, and `tddy-presenter/src/presenter/agent_session_runner.rs`.
30. Grepped the rest of session-lifecycle and daemon-rpc. Read `tddy-session-split/src/service_util.rs` (`write_claude_hooks_settings`), `split_session.rs` (`NATIVE_FILESYSTEM_TOOLS`) and `agent_argv.rs`.
31. Grepped `tddy-tools` for env-var consumers; read `pty_relay.rs` defaults.
32. Enumerated every `"TDDY_*"` literal across the launch crates.
33. Grepped `tddy-tools` transport selection (`TDDY_SOCKET` and others).
34. Read `tddy-agent-launch/src/tool_spawn_plan.rs` and `harness_profile_start.rs`.
35. Grepped the sandboxed starts (`svc_start_sandboxed_{claude,cursor,cli_harness}`, `cli_harness_jail.rs`).
36. Grepped vendor hostnames and port 1455, which led to the Codex OAuth relay crates.
37. Grepped vendor credential env vars (`CLAUDE_CODE_OAUTH_TOKEN`, keychain) and the `codex_oauth` file list.
38. Peeked at `tddy-acp`, `tddy-acp-stub` and `tddy-model-registry`: no coding-agent special-casing found.
39. Listed the tddy-web TS files that carry agent ids (not deep-dived).
40. Grepped the `.proto` files; read the StartSessionRequest field list.
41. Read `tddy-daemon/src/runtime/env.rs`, `provider_route_support.rs` and the `tddy-accounts` `default_for`.
42. Read `tddy-agent-launch/src/opencode_cli_spawn.rs`, `cli_resume_token.rs` and grepped `cli_route_launch.rs`.
43. Grepped `notify_env`, `managed_launch` and `jail_env_builders` env vars.
44. Read `tddy-session-agents/src/coding_agent.rs` (head) and located the `withdrawal_session_meta` callers.
45. Grepped the stream parsers for event-type vocabulary.
46. Read `tddy-changeset` merge, `tddy-worktree-service/src/branch_intent.rs`, `tddy-terminal-rpc/src/pty_runtime.rs`, `daemon-rpc .../notify.rs` and `session_deletion.rs`.
47. Located the `--agent` argv builders (`tddy-spawn/src/spawner.rs`, `tddy-daemon-sandbox/src/action_service.rs`).
48. Grepped repo YAML for `harness:`, `allowed_agents:` and `claude_cli:`; read `dev.daemon.yaml` lines 125–175 and the head of `docs/ft/daemon/harness-profiles.md`.

#### 2. Inspected files (with the excerpts that mattered)

#### tddy-agent-backend (the agent knowledge core; re-exported via `tddy-core`)

**`src/backend/mod.rs`**
- Lines 42–52: `enum AnyBackend { Claude, ClaudeAcp, Cursor, Codex, CodexAcp, Stub, OpenCode }`. Each trait method dispatches over it with a hand-written match (lines 106–166).
- Lines 818–850:
  ```rust
  pub trait CodingBackend: Send + Sync {
      async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError>;
      fn name(&self) -> &str;
      fn submit_channel(&self) -> Option<&SubmitResultChannel> { None }
      fn action_invoke_cache_eligible(&self) -> bool { true }
      async fn list_models(&self) -> Result<BackendModels, BackendError> { Ok(curated_models_for_agent(self.name())) }
      fn context_globs(&self) -> &'static [&'static str] { context_globs_for_agent(self.name()) }
  }
  ```
- `SharedBackend` (Arc wrapper).
- `InvokeRequest` (433–468) carries Claude-shaped fields:
  - `session: SessionMode::{Fresh,Resume}`, documented as "Claude uses `--session-id`/`--resume`; Cursor uses only `--resume`".
  - `extra_allowed_tools`, `socket_path` (sets `TDDY_SOCKET`), `session_dir` (sets `TDDY_SESSION_DIR`/`TDDY_REPO_DIR`), `remote: RemoteToolEnv` (sets `TDDY_REMOTE_*`, lines 392–428).
- `path_with_exe_dir()` (508): PATH is prefixed with the exe dir so `tddy-tools` resolves.
- Agent menus and defaults:
  - `backend_selection_question()` (527–564): the 7 labels.
  - `backend_from_label` (620–631): label to (id, default model).
  - `preselected_index_for_agent` (789–800).
  - `default_model_for_agent` (662): cursor `composer-2.5`, codex/codex-acp `gpt-5`, opencode `fireworks/kimi-k2`, anything else `opus`.
- Model lists:
  - `CLAUDE_MODEL_ALIASES`, `CLAUDE_PINNED_MODELS`, `CLAUDE_DEFAULT_MODEL="opus"`, `OPENCODE_DEFAULT_MODEL`.
  - `curated_models_for_agent` (713), `claude_cli_models()` (734), `cursor_cli_models()` (745, hard-coded list), `acp_models_from_session_state` (766).
- `gather_session_usage` (256): reads the Claude transcript under `<claude_home>/.claude/projects` plus `<session_dir>/egress/accounting.json`.

**`src/provider_route.rs`**
- Lines 22–45: `enum Harness { Claude, ClaudeAcp, ClaudeCli, Codex, CodexAcp, OpenCode, OpenCodeCli, MuseCli, WarpCli, Cursor, CursorCli }`.
- `id()` (57) gives the string ids; `from_agent_or_session_type` (51); `product()` gives display names.
- `route_supported` (214) is the protocol capability matrix: Claude needs anthropic-messages; Codex needs openai-responses; OpenCode takes responses or chat; Muse is Meta only; Cursor and Warp cannot be routed.
- `route_body` (275).
- `claude_code_route` (364): sets `ANTHROPIC_BASE_URL`, pins `ANTHROPIC_MODEL`, `ANTHROPIC_DEFAULT_{OPUS,SONNET,HAIKU}_MODEL` and `CLAUDE_CODE_SUBAGENT_MODEL`; secret `ANTHROPIC_AUTH_TOKEN`; args `--model`.
- `opencode_route` (320) and `opencode_config` (465): generate `opencode.json` with provider `tddy-route`, `@ai-sdk/openai` or `@ai-sdk/openai-compatible`, `{env:TDDY_OPENCODE_PROVIDER_KEY}`; args `--model tddy-route/<m>`.
- `muse_route` (347): secret `META_API_KEY`.

**`src/provider_route/codex.rs`**: `codex_route` emits `-c model_provider="tddy-route"`, `-c model_providers.tddy-route.{name,base_url,env_key,wire_api}` and `-m <model>`. Secret `TDDY_ROUTE_API_KEY`. `CODEX_ROUTE_PRESET_NAMES` = meta, openai-compatible.

**`src/provider_route_file.rs`**: `provider-route.json` (0600) is how the route travels to `tddy-coder --provider-route-path`.

**`src/backend/routed.rs`**
- `AnyBackend::with_provider_route` refuses Cursor and Stub.
- `route_child_env`.
- `splice_route_args` (the route's `--model` replaces the request's).
- `codex_acp_route_args` rewrites `-m X` into `-c model="X"`.

**`src/backend/claude.rs`** (headless `claude -p`)
- `build_claude_args` (69–139):
  ```
  -p <prompt> --output-format stream-json --verbose
  [--permission-mode plan|acceptEdits] [--model]
  [--session-id|--resume]
  [--append-system-prompt-file|--append-system-prompt]
  --allowedTools…  --permission-prompt-tool  --mcp-config
  ```
- `create_mcp_config_temp_file` (166) writes `{"mcpServers":{"tddy-tools":{"command":…, "args":["--mcp"]}}}` to `$TMP/tddy-mcp-*.json`.
- `permission_prompt_tool = "mcp__tddy-tools__approval_prompt"` (346).
- Env (405–424): PATH, route env, `TDDY_SOCKET`, `TDDY_REPO_DIR`, `TDDY_SESSION_DIR`, `TDDY_REMOTE_*`.
- Non-zero exit is tolerated when `<structured-response` is in the output (586).
- Default binary `"claude"` (263); `fn name() -> "claude"`; `PermissionMode` enum (45).

**`src/claude_argv.rs`**: `build_claude_base_argv(binary, model, session_id, permission_mode, dangerously_skip_permissions, resume)` produces `--model`, `--resume|--session-id`, and `--dangerously-skip-permissions|--permission-mode`. Shared by the sandboxed runner, the host agent and `claude_cli_argv`.

**`src/cli_harness.rs`**: an existing declarative-ish registry.
```rust
pub struct CliHarness { session_type, label, route_harness: Harness, context_agent, default_binary,
    binary_env, route_config_env: Option<&str>, argv: fn(&CliLaunch)->Vec<String>, models: CliModelSource }
static CLI_HARNESSES: [CliHarness; 5] = [claude-cli(claude, TDDY_CLAUDE_BINARY), cursor-cli(agent, TDDY_CURSOR_AGENT),
    opencode-cli(opencode, TDDY_OPENCODE_CLI, route_config_env OPENCODE_CONFIG, ListedByBinary(["models"])), MUSE_CLI, WARP_CLI];
```
- argv functions:
  - `claude_cli_argv`: default permission mode `auto`, positional prompt.
  - `cursor_cli_argv`: `--resume <chat>` only when non-blank, `--model`, positional prompt.
  - `opencode_cli_argv`: `--continue` on resume, else `--model`, `--prompt`.
- `cli_harness_requires_model`: claude-cli and cursor-cli.
- `models_from_listing`.

**`src/cli_harness_muse.rs`**: `MUSE_ROUTE_KEY_ENV = "META_API_KEY"`, models `muse-spark-1.3` and `muse-wonder-1.0`, argv `--continue|--model|prompt`, binary env `TDDY_MUSE_CLI`, `context_agent "muse"`.

**`src/cli_harness_warp.rs`**: `warp [--resume <token>]`, no model and no prompt; `resume_token_from_output()` scrapes `warp --resume <TOKEN>` from PTY bytes after stripping ANSI; binary env `TDDY_WARP_CLI`.

**`src/claude_hooks.rs`**
- `build_claude_hooks_settings` builds the `.claude/settings.local.json` `hooks` object for 8 events: SessionStart, UserPromptSubmit, PostToolUse, Notification, PermissionRequest, PreToolUse (matcher `AskUserQuestion`), Stop, SessionEnd.
- Each hook runs `'tddy-tools' session-hook --session … --daemon … [--daemon-socket] --os-user … --hook-token … --event X`. Waiting events get `--await-answer --await-timeout-secs` and a `timeout` field.
- `shell_quote`; `AWAIT_CLIENT_MARGIN_SECS=5`; `CLAUDE_HOOK_TIMEOUT_MARGIN_SECS=10`.

**`src/cursor_hooks.rs`**: `.cursor/hooks.json` with `{"version":1,"hooks":{sessionStart,beforeSubmitPrompt,postToolUse,stop,sessionEnd}}`, same `session-hook` command.

**`src/backend/cursor.rs`**
- `DEFAULT_CLI_BINARY="agent"`.
- `list_models_sync` runs `agent --list-models` and `parse_cursor_model_list` parses `"<id> - <label> (current, default)"`.
- `build_cursor_cli_args` (176): `[--plan] [--resume id] -p <prompt> [--model] --output-format stream-json --stream-partial-output --force --trust --approve-mcps`. The system prompt is prepended to the user prompt because there is no system-prompt flag.
- `register_cursor_mcp_config` writes `<cwd>/.cursor/mcp.json`.
- Same `TDDY_*` env set as Claude.

**`src/backend/codex.rs`**
- `codex exec [-C dir] -s read-only|workspace-write --json [resume] [-m model|route args] [id] <prompt>` (`build_codex_exec_argv`, 422).
- `CODEX_THREAD_ID_FILENAME="codex_thread_id"`, `CODEX_OAUTH_AUTHORIZE_URL_FILENAME`.
- `spawn_oauth_login`: `codex login` with `BROWSER=<current exe>` and `TDDY_CODEX_OAUTH_OUT`.
- Reuses the `claude_nonzero_exit_ok_if_structured_response` hint (318).
- `codex/login_text.rs` holds auth remediation text that mentions `OPENAI_API_KEY`.

**`src/backend/codex_acp.rs`**
- `DEFAULT_CODEX_ACP_BINARY="codex-acp"`; env `TDDY_CODEX_ACP_CLI` and `TDDY_CODEX_CLI`.
- `CodexLoginHooks` implements `AcpAgentHooks::on_session_error`. On an auth-ish error (`oauth.rs`: "auth|unauthorized|login|credential") it runs `codex login` and retries once.
- Resume uses ACP `load_session` with the stored codex thread id.

**`src/backend/acp.rs`**: `ClaudeAcpBackend::default()` is `bunx claude-agent-acp`. The route's env goes to the child and its args are not passed. `name() -> "claude-acp"`.

**`src/backend/opencode_acp.rs`**: `opencode acp`; `OPENCODE_CONFIG` points at an owner-only temp dir holding the route's `opencode.json`; the route's `--model` is not passed; the prompt is merged via `merge_codex_prompt`.

**`src/backend/headless_driver.rs`**
- `enum HeadlessHarness { Claude, Cursor, Codex, Oz }` with ids claude/cursor/codex/oz.
- `HeadlessCommand { program, env, cwd, model, mcp_config, permission_prompt_tool (claude only), disallowed_tools, profile (oz only), mcp_server (codex only) }`.
- `trait ConversationDriver` (377).

**`src/backend/headless_driver/argv.rs`**: per-harness turn argv.
- Claude: `-p P --output-format stream-json --verbose --model --resume --permission-prompt-tool --mcp-config --disallowedTools…`.
- Cursor: `--resume -p --model --output-format stream-json --stream-partial-output --trust --approve-mcps`. The test pins that it never passes `--force`, which the CursorBackend does pass.
- Codex: `exec -c mcp_servers.<n>.{command,args,env_vars}=… -C -s workspace-write -m --json [resume <thread>] P`.
- Oz: `agent run -p P --output-format ndjson -C --conversation --model --mcp --profile`.
- Stream parsers sit beside it: `stream_json.rs` (claude/cursor), `codex_jsonl.rs`, `oz_ndjson.rs`, `transcript.rs`.

**`src/backend/acp_driver.rs`** and `acp_driver/*`
- Generic ACP client: `AcpAgentCommand {program,args,env,cwd}`, `McpServerSpec`, `OpenSession{resume, mcp_servers, _meta}`.
- Traits: `PermissionHandler`, `AcpAgentHooks`.
- `session.rs` calls `session/new` or `session/load` with `mcp_servers`.

**`src/backend/model_catalog.rs`**
- `CLAUDE_CLI_AGENT="claude-cli"`, `CURSOR_CLI_AGENT`.
- `BackendCliPaths { cursor_cli_path, opencode_cli_path, claude_acp_cli_path, codex_acp_cli_path }`.
- `build_backend()` (144) is a second agent-id to `AnyBackend` match.
- `resolve_agent_models` / `list_by_binary`.

**`src/backend/context_globs.rs`**: per-agent repo context allow-list.
- claude/claude-acp: `CLAUDE.md`, `.claude/**`, `.mcp.json`.
- cursor: the Claude set plus `.cursor/**`.
- codex: `.codex/**`.
- opencode: `opencode.json`, `.opencode/**`.
- muse: `.muse/**`.
- warp: `WARP.md`.
- Shared base for all: `AGENTS.md`, `.agents/**`.

**`src/stream/{claude,codex,cursor}.rs`**: NDJSON parsing.
- Claude: system, assistant, user, result, tool_use, tool_result, session_id.
- Codex: thread.started, item.completed, turn.failed, thread_id.
- Cursor: system, assistant, tool_call, result.
- `stream/mod.rs` parses the `AskUserQuestion` input format (`multiSelect`, `allowOther`).

**`src/backend/claude/transcript_usage.rs`**: reads `<claude_home>/.claude/projects/<enc-cwd>/<session_id>.jsonl` and `<session_id>/subagents/agent-*.jsonl` plus `.meta.json` (`agentType`).

**`src/backend/stub.rs`, `mock.rs`**: Stub (`"stub"`, magic prompts SKIP_QUESTIONS/FAIL_*) and Mock (`"mock"`, FIFO queue, `action_invoke_cache_eligible=false`).

#### tddy-workflow / tddy-workflow-recipes

- **`tddy-workflow/src/hints.rs`**: `GoalHints { permission: PermissionHint::{ReadOnly,AcceptEdits}, allowed_tools, agent_cli_plan_mode ("Cursor --plan, Claude --permission-mode plan"), claude_nonzero_exit_ok_if_structured_response }`. A Claude-named field lives in the generic workflow vocabulary.
- **`tddy-workflow-recipes/src/permissions.rs`**: allowlists written in Claude tool vocabulary (`Read`, `Glob`, `Grep`, `SemanticSearch`, `AskUserQuestion`, `ExitPlanMode`, `Bash(tddy-tools *)`, `Agent`, `Edit`, `Write`), plus `build_remote_allowlist` (`mcp__tddy-tools__*`).
- Recipes setting `claude_nonzero_exit_ok_if_structured_response`: `tdd_small/recipe.rs:87`, `grill_me/mod.rs:75,84`, `review/mod.rs:89`, `merge_pr/mod.rs:93,105`.

#### tddy-coder / tddy-coder-cli (tool sessions: `tddy-coder --agent X`)

- **`tddy-coder/src/run.rs`**
  - `verify_tddy_tools_available` (182): skipped for `stub` and `claude-acp`.
  - `create_backend` (2522): specialized agent defs first, then a match on `cursor|claude-acp|codex|codex-acp|opencode|stub|claude`.
  - `BUILTIN_BACKEND_AGENT_IDS` (2618).
  - `resolve_cursor_agent_binary` (`--cursor-agent-path`, `TDDY_CURSOR_AGENT`, `agent`).
  - `agent_activity_source_for("cursor")="cursor-cli"` (310).
  - Default agent `"claude"` (1607).
  - Args (640–830): `--agent` (free string), `--agent-def-path`, `--provider-route-path`, `--model` ("for Claude Code CLI"), `--allowed-tools`, `--cursor-agent-path`, `--codex-cli-path` (env `TDDY_CODEX_CLI`), `--codex-acp-cli-path` (env `TDDY_CODEX_ACP_CLI`), `--opencode-cli-path` (env `TDDY_OPENCODE_CLI`), `--codex-oauth-login`, `--remote` with the remote daemon URL and token.
- **`tddy-coder/src/codex_binaries.rs`**: `resolve_codex_binary`, and `resolve_codex_acp_binary`, which prefers `codex-acp` sitting next to the resolved `codex`.
- **`tddy-coder/src/config.rs`**: coder-config yaml fields `codex_acp_cli_path`, `opencode_cli_path`, and similar.
- **`tddy-coder/src/remote.rs:45`**: the system prompt tells the agent to use the `mcp__tddy-tools__*` tools.
- **`tddy-coder-cli/src/main.rs:9-25`**: pre-main hook. When `TDDY_CODEX_OAUTH_OUT` is set, the binary acts as `BROWSER` and writes the authorize URL.
- **`tddy-coder/src/acp_host.rs:66`**: `tddy-coder --acp --agent …` (tddy exposing itself as an ACP agent).

#### tddy-daemon-kernel (daemon.yaml schema and binary resolution)

**`src/config/agent_cli.rs`**
- `ClaudeCliConfig { binary_path="claude", tddy_tools_path, daemon_url, claude_home_dir, fullscreen=true, answer_wait_secs=120 }`.
- `CursorCliConfig { binary_path="agent", tddy_tools_path, daemon_url, cursor_home_dir }`.
- `CliHarnessConfig { binary_path }` under `cli_harnesses.<session_type>`.
- `CLAUDE_BINARY_ENV="TDDY_CLAUDE_BINARY"`, `CURSOR_BINARY_ENV="TDDY_CURSOR_AGENT"`, `CLAUDE_FULLSCREEN_ENV="CLAUDE_CODE_NO_FLICKER"` (via `claude_cli_renderer_env`), `CLAUDE_HOME_ENV="TDDY_SANDBOX_CLAUDE_HOME"` (default `~/.tddy/sandbox-claude-home`), `CURSOR_HOME_ENV="TDDY_SANDBOX_CURSOR_HOME"`.
- `resolve_claude_binary_path` and `resolve_cursor_binary_path`: env, then config path, then `~/.local/bin/<name>`, then PATH skipping `.superset/bin` shim dirs (`is_wrapper_shim_dir`), then canonicalize.
- `find_real_harness_on_host`, `resolve_harness_binary_name`, `cli_harness_binary`.
- `resolve_cursor_cli_{tddy_tools_path,daemon_url}` falls back to the `claude_cli` values.

**`src/config.rs`**
- `allowed_agents: Vec<AllowedAgent{id,label}>` (105).
- `codex_oauth_loopback_proxy_eligible` (149).
- `claude_cli` (155), `cursor_cli` (158), `cli_harnesses` (`BTreeMap`).

**`dev.daemon.yaml`** (repo root, lines 131–167): `allowed_agents` lists claude, claude-acp, cursor, stub, codex, codex-acp, opencode; the `claude_cli:` section is commented out.

#### tddy-agent-launch (daemon-side session start, carved from session-lifecycle)

- **`src/svc_start_session_core.rs`**
  - 76–98: `req.agent` is checked against the registry (`agent_def_for_spawn`) or `config.allowed_agents()`.
  - 382: `if session_type=="claude-cli"`, with `req.sandbox` choosing `start_sandboxed_claude_cli_from_request` vs `start_claude_cli_from_request`.
  - 421: the same for `"cursor-cli"`.
  - 459: `cli_harness(session_type)` takes any other registered harness to `start_cli_harness`.
  - Anything else is a tool session (`spawn_tool_session`, which spawns `tddy-coder --agent …`).
- **`src/claude_cli_spawn.rs`**: requires a model; `start_route(..., Harness::ClaudeCli, ...)` (379); `install_claude_cli_hooks`; `resolve_start_session_claude_binary`; managed launch env; `notify_env`; `env_extra.extend(route.env)`; `claude_cli_renderer_env`.
- **`src/claude_cli_spawn_steps.rs:168-181`**: writes `.claude/settings.local.json` via `tddy_session_split::service_util::write_claude_hooks_settings`.
- **`src/chat.rs`**: `install_cursor_hooks_in_worktree` writes `.cursor/hooks.json`; `cursor_notify_env`. Line 30 has a TODO: no `daemon_socket` for cursor hooks.
- **`src/cursor_cli_spawn.rs`**
  - `prepare_cursor_cli_launch` (317) writes `.cursor/rules/tddy-managed-workflow.mdc` for managed recipes.
  - `mint_cursor_chat_id`.
  - `SessionMetadata.cursor_chat_id`.
- **`src/resume.rs`**: cursor resume via `resolve_cursor_binary_path`.
- **`src/svc_start_sandboxed_claude_cli_session.rs`**
  - 323–390: `resolve_claude_binary_path`, `resolve_claude_home_dir`, `prepare_persistent_claude_home` (seed creds, local install mirror), `pick_free_loopback_port` for the egress shim.
  - Runner argv: `--session-id --context-dir --tool-ipc-socket --tddy-tools-path --ready-marker --claude-binary --model --permission-mode --egress-shim-port --stdio [--dangerously-skip-permissions] [--append-system-prompt-file] [--claude-arg …]`.
  - 486: `env.extend(claude_cli_renderer_env())`.
- **`src/svc_start_sandboxed_cursor_cli_session.rs`**: 250 writes the rules dir; 270 `resolve_cursor_binary_path`; runner argv uses `--agent-kind cursor --agent-binary … --agent-arg`.
- **`src/svc_start_sandboxed_cli_harness_session.rs`** with **`src/cli_harness_jail.rs`**: opencode and muse in the jail.
  - `require_provider_route`: a jailed opencode or muse session must be routed.
  - `cli_harness_runner_argv` uses `--agent-kind <kind> --agent-binary --model --egress-shim-port --stdio [--resume]`.
  - `resolve_jail_agent_binary`.
- **`src/svc_relaunch_sandboxed_runner.rs`** (163–201) and **`src/relaunch_jail_steps.rs:72`**: the same Claude runner argv on relaunch.
- **`src/svc_resume_claude_cli_session.rs`**: resume binary, `Harness::ClaudeCli` route, renderer env.
- **`src/opencode_cli_spawn.rs`** (opencode-cli and muse-cli)
  - `harness_binary()`: `binary_env`, then `cli_harnesses.<t>.binary_path`, then `default_binary`, then host resolution.
  - The module doc says "never sandboxed", which contradicts the sandboxed path above; the doc looks stale.
- **`src/cli_route_launch.rs`**: writes route files 0600 into the session dir and sets `route_config_env` (`OPENCODE_CONFIG`).
- **`src/cli_resume_token.rs`**: Warp resume-token `ExitHook`, stored as `SessionMetadata.harness_resume_token`.
- **`src/harness_profile_start.rs:21-38`**: `Harness` to (session_type or agent), split into CLI-session vs tool-session.
- **`src/tool_session_route.rs:37`**: `Harness::from_agent_or_session_type(agent)`.
- **`src/managed_launch.rs:74-82`**: `TDDY_SOCKET`, `TDDY_SESSION_DIR`, `TDDY_REPO_DIR`.
- **`src/jail_env_builders.rs`**: `TDDY_SUBAGENT`, `TDDY_SUBAGENTS_JSON`, `TDDY_DAEMON_INSTANCE_ID`, `TDDY_LSP_TOOLS=rust`.
- **`src/notify_env.rs`**: `agent-system-prompt.md` (notify guidance), with hook token and OS user env (`TDDY_HOOK_TOKEN` and `TDDY_OS_USER`, defined in `tddy-service/src/session_activity.rs:42-44`).
- **`src/project_home_launch.rs:133`**: project-home sessions are claude-cli only, using `--append-system-prompt-file`.
- **`src/hooks_and_urls.rs:146`**: `resolve_resume_session_claude_binary`.

#### tddy-session-lifecycle

**`src/coding_agent_launcher.rs`**: daemon `CodingAgentLauncher` for coding subagents.
- Constants: `TDDY_CODEX_ACP_CLI`/`codex-acp`, `TDDY_CLAUDE_AGENT_ACP_CLI`/`bunx claude-agent-acp`, `opencode acp`, `oz`, `mcp.json` (0600), `APPROVAL_PROMPT_TOOL`.
- `harness_command` (279) is a match over CLAUDE_ACP, CODEX_ACP and OPENCODE.
- `apply_route` (305): codex-acp gets `-c` args; opencode gets inline config via `OPENCODE_CONFIG_CONTENT`.
- `command_before_user_switch` calls `refuse_unless_confinable` and applies `opencode_withdrawal_config`.
- `headless_plan` (405–495), per harness:
  - Claude: owner-only `mcp.json` in `<data>/headless/<conv>/` plus `--permission-prompt-tool`.
  - Oz: top-level servers JSON.
  - Cursor: `write_cursor_project_mcp_config`, which writes `<worktree>/.cursor/mcp.json` and adds it to `.git/info/exclude`; an existing file is refused.
  - Codex: inline `-c mcp_servers` with the env forwarded.
- `headless_program`: claude and cursor use the daemon resolvers; codex and oz are bare names.
- `headless_route_env` refuses routes that carry files or args.
- `route_from_vault`.

`coding_agent_as_user.rs`, `coding_agent_relay.rs`: `setpriv` / supervisor relay (generic).

#### tddy-coding-subagent

- **`src/catalog.rs`**: `CLAUDE_ACP`, `CODEX_ACP`, `OPENCODE`, `ACP_HARNESSES`. `coding_agent_defs` comes from harness profiles plus ACP `allowed_agents`; the model defaults via `default_model_for_agent`.
- **`src/transports.rs`**
  - `HEADLESS_HARNESSES = ["claude","cursor","codex","oz"]`.
  - `Transport::{Acp, Headless}`.
  - `permission_policy_of`: ACP and claude ask the caller; cursor is "read-only; can't ask"; codex is "workspace-write sandbox"; oz is "profile decides".
- **`src/confinement.rs`**
  - `confinement_for`: claude-acp and opencode can withdraw native tools; everything else cannot.
  - `refuse_unless_confinable`.
  - `CLAUDE_NATIVE_TOOLS`; `withdrawal_session_meta` sets ACP `_meta.claudeCode.options.disallowedTools`.
  - `OPENCODE_NATIVE_TOOLS`; `opencode_withdrawal_config` sets `{"tools":{bash:false,…}}`.
  - `OPENCODE_CONFIG_CONTENT`.
- **`src/launcher.rs`**: `TDDY_TOOLS_SERVER="tddy-tools"`, `--mcp`, `CONVERSATION_ID_ENV="TDDY_CONVERSATION_ID"`, `SESSION_TOKEN_ENV="TDDY_REMOTE_SESSION_TOKEN"`, `HEADLESS_DIR`, `trait CodingAgentLauncher` (131), `enum AgentTransport`, `struct CodingAgentLaunch` (94).

#### tddy-session-agents

- **`src/coding_agent.rs`**: `coding_agent_defs_for(tddy_data_dir, config)`; applies `withdrawal_session_meta` at 203.
- **`src/session_agent_roster.rs:429`**: `"claude-cli"` (test fixture).

#### tddy-discovery

- **`src/harness_profile.rs`**: `HarnessProfile { name, label, harness, provider_account, model }` loaded from `<tddyhome>/agents/*.yaml` whenever a top-level `harness:` key exists (`deny_unknown_fields`). Documented in `docs/ft/daemon/harness-profiles.md`.
- **`src/agent_def.rs:107`**: `SpecializedAgentDef { name, label, model, base_url, api_key, system_prompt(_path), usage_notes, tools, max_turns, replaces }`. This is the OpenAI-loop agent, not a coding CLI.

#### tddy-sandbox-recipes (jail recipes per agent)

- **`src/plan.rs`**
  - `enum SandboxRecipe { Generic, ClaudeCli, CursorCli, Shell, RunnerPty, OpenCodeCli, MuseCli }`.
  - `detect_recipe_from_argv` sniffs `--agent-kind cursor|opencode|muse`, `--claude-binary` and `--pty-command`.
  - `parse_recipe_name`; `agent_binary_from_runner_argv`.
  - `collect_reads`, `policy_for_recipe` and `copies_for_recipe` all match per recipe.
  - `finish_builder` (285): for ClaudeCli without a route, injects host `CLAUDE_CODE_OAUTH_TOKEN` as a secret.
  - `build_runner_plan`: route env, `route_config_env` file materialisation, `allow_oauth_inbound = !routed && (ClaudeCli|CursorCli)`.
- **`src/claude_cli.rs`**
  - `seed_claude_credentials` copies `~/.claude/.credentials.json` once (0600).
  - `seed_claude_local_install` mirrors `~/.local/bin/claude` and `~/.local/share/claude/versions/<v>`.
  - `claude_credentials_copies`.
  - `claude_interactive_policy` (JIT, fork, mach all, PTY, exec paths `/usr/bin…/opt/homebrew`).
  - `claude_runner_env_overlay` sets `CLAUDE_CODE_TMPDIR` and `CLAUDE_TMPDIR`.
  - `PERMISSION_PROMPT_TOOL`; `SUBAGENT_TOOLS` (`mcp__tddy-tools__subagent_*`).
  - `native_aliases` (Shell to Bash/BashOutput/KillShell; Write to Edit/MultiEdit/NotebookEdit).
  - `effective_replaced_tools` (SemanticSearch); `build_claude_allowlist` and `build_claude_disallowlist`.
  - `write_claude_mcp_config` writes `claude-mcp-config.json` with an `env` block.
  - `NATIVE_EXEC_TOOL_NAMES`, `build_host_agent_disallowlist`, `append_host_agent_mcp_args`, `append_claude_mcp_args` (ends with the variadic `--mcp-config`), `claude_scratch_mcp_dir` (`$TMPDIR`).
- **`src/cursor_cli.rs`**
  - `cursor_agent_prerequisite_reads` (install dir, bundled `node`/`index.js`, `.../cursor-agent` share root, `~/.local/bin`, `~/.local/share`).
  - `seed_cursor_local_install` (`~/.local/share/cursor-agent/versions/<v>`).
  - `cursor_interactive_policy` is the Claude policy.
  - `seed_cursor_credentials` copies `~/.cursor/{cli-config.json,auth.json,mcp.json}`; `cursor_credentials_copies` copies `auth.json` and `cli-config.json`.
  - `write_cursor_mcp_config` writes `<base>/.cursor/mcp.json`.
  - `build_cursor_sandbox_argv` rewrites `agent` to `<install>/node <install>/index.js` when both exist.
  - `cursor_runner_env_overlay` sets `CURSOR_TMPDIR`, `CURSOR_SKIP_KEYCHAIN=1` and `CI=true`.
- **`src/opencode_cli.rs`**: Claude policy plus exec reads.
- **`src/muse_cli.rs`**: Claude policy with `allow_dynamic_code_generation:false` (TODO: unverified).
- **`src/route_files.rs`**: `provider-route/` dir 0700, files 0600; `route_config_env(recipe)` is looked up through `cli_harness("opencode-cli"|"muse-cli")`.

#### tddy-sandbox-runner (in-jail runner)

- **`src/runner.rs`**
  - Args (286–345): `--claude-binary` (default claude), `--agent-kind` (`claude|cursor|opencode|muse`), `--agent-binary`, `--model`, `--permission-mode auto`, `--dangerously-skip-permissions`, `--resume`, `--append-system-prompt-file`, `--claude-arg`, `--agent-arg`, `--egress-shim-port`, `--pty-command`, `--mcp-log-level`.
  - `run_claude_pty_thread` (1244) sets `TERM`, `TDDY_EGRESS_SHIM`, `HTTPS_PROXY` and `HTTP_PROXY` to the egress shim, passes the `TDDY_EGRESS_PROBE_*` vars, and injects `TDDY_SECRET_*` file secrets as env (1070).
  - `spawn_claude_pty` (1512) uses `build_claude_base_argv` plus `append_claude_mcp_args`, reads `TDDY_SUBAGENT`, `TDDY_SUBAGENTS_JSON`, `TDDY_SUBAGENT_REPLACES` and `TDDY_SEMANTIC_INDEX_DB`, and sets `TDDY_TOOLS_LOG_FILE` and `TDDY_TOOLS_ACCOUNTING_FILE` under `TDDY_SANDBOX_EGRESS_DIR`.
  - `spawn_cursor_pty` (1660) uses `build_cursor_sandbox_argv`.
  - Dispatch at 2467–2518: opencode and muse go to `harness_pty`, then cursor, then the claude default.
- **`src/harness_pty.rs`**: `hosted_harness(agent_kind)` maps "opencode" to `opencode-cli` and "muse" to `muse-cli` through the `CliHarness` registry.

#### tddy-sandbox-app (standalone sandbox CLI)

- **`src/spawn.rs`**
  - `enum AgentKind { Claude, Cursor, OpenCode, Muse }` (39); `unstarted_agent_kind` refuses OpenCode and Muse.
  - Its own copies of `seed_claude_credentials` (173), `seed_claude_local_install` (204) and `seed_cursor_credentials` (375) duplicate the recipes.
  - `resolve_cursor_binary` (425), `resolve_claude_binary` (462).
  - `TDDY_SUBAGENT*` env (146–164).
- **`src/host_agent.rs:126-197`**: `build_host_agent_argv` (sandboxed-codebase mode, Claude only) uses `build_claude_base_argv`, then `--strict-mcp-config --setting-sources user`, then `append_host_agent_mcp_args`. Env `TDDY_SANDBOX_TOOL_IPC`, `TDDY_TOOLS_LOG_FILE`, `TDDY_TOOLS_ACCOUNTING_FILE`.
- **`src/main.rs`**: `--agent-kind` (default claude); `--claude-home-dir` (env `TDDY_SANDBOX_CLAUDE_HOME`); `--cursor-home-dir` (env `TDDY_SANDBOX_CURSOR_HOME`); `TDDY_SANDBOX_CODEBASE_HOME`; `refuse_unservable_sandboxed_session` (Cursor cannot use sandboxed codebase mode); line 938 locates the host agent transcript under `~/.claude`.
- **`src/config.rs`**: `claude_args`, `claude_binary`, `cursor_binary`, home dirs.

#### tddy-daemon-sandbox and tddy-sandbox

- **`tddy-daemon-sandbox/src/sandbox_session.rs`**
  - `build_sandbox_runner_env` (496) always adds the Claude env overlay.
  - `build_sandboxed_cursor_runner_env` (527).
  - 710–719: argv sniffing for `--claude-binary` and `--agent-kind cursor`.
  - `prepare_persistent_claude_home` (837) and `prepare_persistent_cursor_home` (864).
  - `prepare_context_dir(globs)`.
- **`tddy-daemon-sandbox/src/sandbox_plan_builder.rs:134`**: Claude env overlay.
- **`tddy-daemon-sandbox/src/action_service.rs:50`**: action kind `"claude-cli"`; line 199 pushes `--agent`.
- **`tddy-sandbox/src/context_manifest.rs:75`**: `CONTEXT_EXCLUDE_GLOBS = [".claude/settings.local.json"]`.
- **`tddy-sandbox/src/context_dir.rs`**: `MANAGED_CODEBASE_PREAMBLE` prepended to `PREAMBLE_FILES = ["CLAUDE.md","AGENTS.md"]`.
- **`tddy-sandbox/src/builder.rs:744`**: `CLAUDE_CODE_OAUTH_TOKEN` (test).

#### tddy-session-split

- **`src/service_util.rs:298-328`**: `write_claude_hooks_settings` writes `<cwd>/.claude/settings.local.json`; `resolve_start_session_claude_binary`.
- **`src/split_session.rs:32`**: `NATIVE_FILESYSTEM_TOOLS` (Claude natives; this duplicates `CLAUDE_NATIVE_TOOLS` in coding-subagent). Split placement is "claude-cli only".
- **`src/agent_argv.rs`**: `split_claude_extra_args`.
- `svc_spawn_split_agent.rs:349,417`, `svc_resume_split_wiring.rs:68`, `svc_start_sandboxed_codebase_session.rs:218`: `context_agent_for_session_type("claude-cli")` is hard-coded.
- **`src/coding_agent_wiring.rs`**: `CodingAgentToolHost` (tool server for coding agents).

#### Hooks and status (runtime protocol of Claude and Cursor hooks)

- **`tddy-tools/src/session_hook.rs`**
  - Parses hook stdin `hook_event_name`, `tool_name`, `tool_input`, `notification_type`.
  - `--await-answer` emits Claude's `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow|deny","updatedPermissions"}}}` (106–137).
  - Handles `PreToolUse`+`AskUserQuestion`.
- **`tddy-changeset/src/session_activity.rs:102-122`**: `activity_status_from_hook_with_tool` maps both Claude (PascalCase) and Cursor (camelCase) event names; `ASK_USER_QUESTION_TOOL`; `HookEvent`.
- **`tddy-tools/src/server.rs:65`**: the `approval_prompt` MCP tool in Claude `--permission-prompt-tool` format.
- **`tddy-session-activity/src/service/hook_reports.rs:175`**: only claude-cli and cursor-cli may report status.
- **`tddy-session-activity/src/service/operator_notifications.rs:39`**: same gate.
- **`tddy-session-activity/src/service/question_answers.rs:69`**: claude-cli only.
- **`tddy-daemon-rpc/src/exec_tool/project_home/notify.rs:26`**: claude-cli or cursor-cli may notify.
- **`tddy-session-activity/src/session_deletion.rs:47`**: `worktree_removal_applies_to` covers claude-cli, opencode-cli, muse-cli, warp-cli and workspace; cursor-cli is excluded on purpose.

#### Model listing (daemon to `tddy-tools list-models`)

- **`tddy-daemon-rpc/src/catalog/agent_models.rs:42`**: path flag per agent (cursor uses `--cursor-cli-path`; opencode and opencode-cli use `--opencode-cli-path`).
- **`tddy-daemon-rpc/src/catalog/ports.rs:112-218`**: cursor's binary resolution special-cased; `listing_harness_session("opencode")="opencode-cli"`.
- **`tddy-tools/src/list_models.rs`**: the CLI side.

#### Other special cases

- **`tddy-presenter/src/presenter/agent_session_runner.rs:27`**: `backend_is_agent_driven` is claude or claude-acp (with `TDDY_AGENT_DRIVEN`).
- **`tddy-presenter/src/usage_watcher.rs:155`**: `include_main_agent = agent=="claude"`.
- **`tddy-cli-sessions/src/cli_session_manager/argv.rs`**: `build_claude_argv` / `build_cursor_argv` wrap the registry. `launch.rs:81,221`: `start_cursor` / `start_claude`.
- **`tddy-worktree-service/src/branch_intent.rs:65-74`**: `BranchIntentPolicy::claude_cli()` refuses a blank branch name; `cursor_cli()` generates one; the prefix is the session type.
- **`tddy-telegram-control/.../callbacks.rs`**: `/start-claude`, `/start-cursor`, `CLAUDE_CLI_MODELS`, `CURSOR_CLI_MODELS`. **`cli_spawns.rs`**: claude-cli and cursor-cli spawns.
- **`tddy-tools/src/pty_relay.rs:103-107`**: default model `CLAUDE_DEFAULT_MODEL`, default session type `claude-cli`.
- **`tddy-terminal-rpc/proto/terminal_session.proto:40`**: the main terminal is named `"claude"`.
- **`tddy-actions/src/catalog.rs:17`**: action kind `"claude-cli"`.
- **`tddy-daemon/src/provider_route_support.rs`**: `HarnessRouteSupport`, which adapts `provider_route` for `tddy-accounts`.
- **`tddy-accounts/src/provider_account.rs:440`**: `META_DEFAULT_FOR` (harness ids an account is the default for).
- **`tddy-daemon/src/provider_model_probe.rs:27`**: the `anthropic-version` header. This is provider probing, not an agent.
- Codex OAuth relay: `tddy-daemon-auth/src/codex_oauth_relay.rs:58` (chatgpt.com, openai.com, auth.openai.com), `tddy-daemon-livekit/src/oauth_loopback_tunnel.rs`, `tddy-daemon-livekit/src/codex_oauth_participant_metadata.rs` (port 1455), `tddy-service/src/codex_oauth_scan.rs`, `tddy-service/src/codex_oauth_validate.rs`, and `tddy-daemon/src/runtime/env.rs` (`TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE`).
- **`tddy-changeset/src/session_metadata.rs:42,47`**: `SessionMetadata.cursor_chat_id` and `harness_resume_token` (Warp) are agent-specific persisted fields; `agent` and `session_type` are generic.
- **`tddy-spawn/src/spawner.rs:914-940`**: tddy-coder argv `--agent`, `--agent-def-path`, `--provider-route-path`.
- **Protos**
  - `tddy-service/proto/session.proto` StartSessionRequest: `agent=4`, `session_type=7` ("tool" default or claude-cli), `model=8`, `permission_mode=14` (claude), `sandbox=16`, `managed_codebase=17`, `claude_args=25`, `dangerously_skip_permissions=26`, `sandboxed_codebase=39`.
  - `catalog.proto`: "claude-cli" pseudo-agent.
  - `activity.proto`: claude-cli and cursor-cli hooks; source "coder|cursor-cli|claude-cli|sandbox".
  - `session_files.proto`: agent context allow-list.
- **tddy-web** (noted only, 34 files mention claude-cli): `src/constants/claudeCliModels.ts`, `src/components/sessions/{cliHarnessOptions.ts, CreateSessionClaudeCliFields.tsx, CreateSessionPermissionFields.tsx, CreateSessionSandboxedCodebaseToggle.tsx, CreateSessionManagedCodebaseFields.tsx, createSessionRequest.ts, useCreateSessionModel.ts, projectHomeSessionTypes.ts, jailedCodebaseHalf.ts}`, `src/utils/sessionDrawerLabel.ts`, `src/components/accounts/ProviderAccountDefaultsControl.tsx`, `src/lib/sessionParticipantMetadata.ts` (codex_oauth).
- No coding-agent special-casing in: `tddy-acp` (tddy as an ACP agent; mapping), `tddy-acp-stub` (test ACP agent standing in for claude-agent-acp/codex-acp), `tddy-model-registry` (OpenAI-loop assistants), `tddy-credentials`, `tddy-credential-sync`, `tddy-zed-extensions`. The tool-engine "cursor" hits are pagination cursors; its `start_session_args/schema.rs:91-181` describes claude-cli-only args (permission_mode, dangerously_skip_permissions, managed/sandboxed codebase).

#### 3. Grep and glob log

| # | Pattern | Scope | Notable hits |
|---|---|---|---|
| G1 | `cursor-agent\|codex-acp\|"codex"\|opencode\|"claude"\|claude-agent-acp` (files, per crate) | `packages/**/*.rs` | 38 crates; top: session-lifecycle 25, agent-backend 25, agent-launch 24, daemon-rpc 16 |
| G2 | `pub (trait\|enum)` | agent-backend/src | Harness, RouteRefusal, RouteChoice, CliModelSource, ToolExecutor, PermissionMode, HeadlessHarness, ConversationDriver, Acp*, PermissionHandler, AcpAgentHooks, AnyBackend, SessionMode, CodingBackend |
| G3 | `Command::new\|"--…"\|.env(\|TDDY_\|CLAUDE_\|ANTHROPIC_\|.claude\|settings.json\|mcp.json` | backend/claude.rs, claude_argv.rs | argv flags listed in §2 |
| G4 | same kind of pattern over cursor, codex, codex_acp, opencode_acp, acp, routed, stub, mock, context_globs, model_catalog, headless_driver, acp_driver | backend/* | `"agent"`, `--list-models`, `.cursor/mcp.json`, `TDDY_CODEX_OAUTH_OUT`, `BROWSER`, `bunx claude-agent-acp`, `OPENCODE_CONFIG`, `-c` route args |
| G5 | `AnyBackend::\|agent literals\|*_cli_path\|TDDY_CODEX\|--agent` | tddy-coder, tddy-coder-cli | run.rs 184, 289–307, 2522–2628; codex_binaries.rs; config.rs |
| G6 | `TDDY_CODEX_OAUTH_OUT` | all .rs | tddy-coder-cli/src/main.rs:9-25 |
| G7 | `^pub (fn\|const)…\|"[A-Z_]+"\|.claude\|Library` | sandbox-recipes/claude_cli.rs | seed/copies/policy/overlay/MCP builders |
| G8 | per-file count `claude\|cursor\|codex\|opencode\|muse\|warp` | agent-launch/src | session_provider_route_tests 63, sandboxed_cli_harness_tests 57, cursor_cli_spawn 33, claude_cli_spawn 30 … |
| G9 | `"[A-Z]+_…"\|"--…"\|.claude/.cursor\|settings/hooks/mcp.json\|resolve_*binary\|Harness::` | agent-launch non-test | runner argv in 3 sandboxed starts, `.cursor/rules`, `.cursor/hooks.json`, Harness matches |
| G10 | `session_type\|"claude-cli"\|…\|cli_harness(` | svc_start_session_core.rs | 382, 421, 459 dispatch |
| G11 | `pub fn resolve_*\|claude_cli\|cursor_cli\|allowed_agents\|struct ClaudeCli…` | daemon-kernel/src | agent_cli.rs, config.rs:105,149,155,158 |
| G12 | agent words, `AgentKind\|agent_kind\|CLAUDE_\|CURSOR_` | sandbox-runner, sandbox-app, sandbox-darwin, daemon-sandbox, sandbox | runner 83, app 244, darwin 0, daemon-sandbox 42, sandbox 26 |
| G13 | `claude\|cursor\|codex\|opencode` | actions, tool-engine, toolcall, workflow*, core | actions catalog; workflow hints; recipe permissions (tool-engine hits are mostly pagination "cursor") |
| G14 | agent id literals, `cursor_chat_id\|harness_resume_token\|Harness::\|HeadlessHarness\|.claude/\|CLAUDE_\|CURSOR_\|ANTHROPIC_` | session-lifecycle, session-split, cli-sessions | coding_agent_launcher.rs, service_util.rs:315, split_session.rs, argv.rs, launch.rs |
| G15 | same as G14 | ~25 remaining crates | daemon-rpc catalog, session-activity gates, changeset metadata, presenter, telegram, tools pty_relay, accounts, worktree-service, terminal-rpc |
| G16 | `hook_event_name\|SessionStart…` | tddy-tools, session-activity | session_hook.rs; changeset session_activity.rs:102 |
| G17 | `"TDDY_[A-Z0-9_]+"` (counted) | launch-related crates | 55 distinct vars (see §4.6) |
| G18 | `anthropic.com\|claude.ai\|cursor.(sh\|com)\|openai.com\|chatgpt.com\|1455` | all .rs | Codex OAuth relay only; no per-vendor egress host allowlist |
| G19 | `CLAUDE_CODE_OAUTH_TOKEN\|CURSOR_API_KEY\|OPENAI_API_KEY\|ANTHROPIC_API_KEY\|keychain` | all .rs | plan.rs:285, cursor keychain-skip tests, codex login text |
| G20 | `codex_oauth` (files) | all .rs | 20 files (daemon-livekit, daemon-auth, service, coder, coder-cli, daemon-kernel, session-lifecycle) |
| G21 | `"--agent"\|"--agent-def-path"\|"--provider-route-path"` | all .rs | tddy-spawn/src/spawner.rs:914-940, action_service.rs:199, acp_host.rs:66 |
| G22 | `^harness:`, `allowed_agents:\|claude_cli:\|cli_harnesses:` | repo YAML | dev.daemon.yaml:131,151; dev.desktop.yaml:110; desktop e2e fixture; no checked-in harness profiles |
| G23 | `claude\|cursor_chat\|permission_mode\|dangerously\|codex\|opencode\|session_type` | *.proto | session.proto, catalog.proto, activity.proto, accounts.proto, session_files.proto, terminal_session.proto |
| G24 | `"claude-cli"…` (files) | tddy-web/src *.ts/tsx | 34 files (listed in §2) |
| G25 | stream event-type literals | stream/*, headless_driver/* | vocabulary listed in §2 |

#### 4. Findings

#### 4.1 Abstraction traits

| Trait | Location | Role |
|---|---|---|
| `CodingBackend` | `tddy-agent-backend/src/backend/mod.rs:818` | Workflow-goal invocation (`invoke`, `name`, `list_models`, `context_globs`, `submit_channel`, `action_invoke_cache_eligible`). Model lists and context globs are keyed by the `name()` string. |
| `ConversationDriver` | `tddy-agent-backend/src/backend/headless_driver.rs:377` | One conversation turn-by-turn, implemented by both ACP and headless. |
| `AcpAgentHooks`, `PermissionHandler` | `tddy-agent-backend/src/backend/acp_driver.rs:172,210` | Per-agent ACP hooks (the Codex login retry; OpenCode route-file lifetime). |
| `CodingAgentLauncher` | `tddy-coding-subagent/src/launcher.rs:131` | Daemon port turning a `CodingAgentLaunch` into an ACP command, a transport, MCP servers or a headless command. Implemented by `DaemonCodingAgentLauncher` in `tddy-session-lifecycle/src/coding_agent_launcher.rs`. |
| `ToolExecutor` | `tddy-agent-backend/src/backend/tool_executor.rs:11` | In-memory (stub) vs process (tddy-tools). |
| `CliHarness` (a fn-pointer struct, not a trait) | `tddy-agent-backend/src/cli_harness.rs` | Descriptor registry for PTY CLI sessions. This is already the closest thing to a declarative tool definition. |

#### 4.2 Enums and string tables that enumerate agents

There are 10 or more parallel ones:

- `AnyBackend` (7 variants): `tddy-agent-backend/src/backend/mod.rs:42`.
- `Harness` (11 variants with string ids): `provider_route.rs:22`.
- `HeadlessHarness` (claude, cursor, codex, oz): `headless_driver.rs:41`.
- `CLI_HARNESSES` (claude-cli, cursor-cli, opencode-cli, muse-cli, warp-cli): `cli_harness.rs`.
- `SandboxRecipe` (Generic, ClaudeCli, CursorCli, Shell, RunnerPty, OpenCodeCli, MuseCli): `tddy-sandbox-recipes/src/plan.rs`.
- `AgentKind` (Claude, Cursor, OpenCode, Muse): `tddy-sandbox-app/src/spawn.rs:39`.
- The runner's `--agent-kind` string (claude, cursor, opencode, muse): `tddy-sandbox-runner/src/runner.rs:290`.
- `ACP_HARNESSES` and `HEADLESS_HARNESSES`: `tddy-coding-subagent/src/{catalog,transports}.rs`.
- `BUILTIN_BACKEND_AGENT_IDS`: `tddy-coder/src/run.rs:2618`.
- `build_backend` match: `model_catalog.rs:144`; `create_backend` match: `tddy-coder/src/run.rs:2551`.
- `backend_selection_question`, `backend_from_label`, `preselected_index_for_agent`, `default_model_for_agent`, `curated_models_for_agent`, `context_globs_for_agent`: all string matches in `backend/mod.rs` and `context_globs.rs`.
- `PermissionMode` (Claude): `backend/claude.rs:45`. `GoalHints.agent_cli_plan_mode` and `claude_nonzero_exit_ok_if_structured_response`: `tddy-workflow/src/hints.rs`.
- Session-type string gates: `svc_start_session_core.rs:382,421`, `hook_reports.rs:175`, `operator_notifications.rs:39`, `question_answers.rs:69`, `notify.rs:26`, `session_deletion.rs:47`, `branch_intent.rs`, `cli_harness_requires_model`, `harness_profile_start.rs:28-38`.

#### 4.3 How a user or config selects an agent today

- **tddy-coder CLI.** `--agent <id|specialized-def-name>` (free string) is resolved in `create_backend`. If omitted, the user gets an interactive `backend_selection_question` (default `claude`). Path flags (`--cursor-agent-path`, `--codex-cli-path`, `--codex-acp-cli-path`, `--opencode-cli-path`) fall back to `TDDY_CURSOR_AGENT`, `TDDY_CODEX_CLI`, `TDDY_CODEX_ACP_CLI`, `TDDY_OPENCODE_CLI`, then to coder-config YAML (`tddy-coder/src/config.rs`). `--agent-def-path` and `--provider-route-path` are 0600 JSON written by the daemon.
- **Daemon StartSessionRequest** (web, Telegram, `tddy-tools` relay, project-home tools):
  - `session_type` picks the spawn path: `claude-cli`, `cursor-cli`, any `CliHarness` (opencode-cli, muse-cli, warp-cli), `workspace`, or `tool`/empty (tddy-coder).
  - `agent` (tool sessions) is validated against `daemon.yaml allowed_agents` or the model-registry assistant.
  - `model`; `permission_mode` and `dangerously_skip_permissions` and `claude_args` (Claude only).
  - `sandbox`, `managed_codebase`, `sandboxed_codebase`, `specialized_agents`.
- **daemon.yaml.**
  - `allowed_agents: [{id,label}]`.
  - `claude_cli: {binary_path, tddy_tools_path, daemon_url, claude_home_dir, fullscreen, answer_wait_secs}`.
  - `cursor_cli: {binary_path, tddy_tools_path, daemon_url, cursor_home_dir}`.
  - `cli_harnesses.<session_type>.binary_path`.
  - `codex_oauth_loopback_proxy_eligible`.
  - Env overrides: `TDDY_CLAUDE_BINARY`, `TDDY_CURSOR_AGENT`, `TDDY_OPENCODE_CLI`, `TDDY_MUSE_CLI`, `TDDY_WARP_CLI`, `TDDY_CLAUDE_AGENT_ACP_CLI`, `TDDY_CODEX_ACP_CLI`, `TDDY_SANDBOX_CLAUDE_HOME`, `TDDY_SANDBOX_CURSOR_HOME`.
- **Harness profiles.** `<tddyhome>/agents/*.yaml` with `harness:` plus `model` and `provider_account` (`tddy-discovery/src/harness_profile.rs`). `profile_start` maps them to a session_type or an agent. They also feed the coding-subagent catalog (`coding_agent_defs`).
- **Specialized agent defs.** `<tddyhome>/agents/*.yaml` without `harness:`, or registry assistants. These are OpenAI-loop agents, not coding CLIs.
- **Provider route choice.** Session account, then agent definition, then the user's `default_for` account (`provider_route::choose_route`, `tddy-accounts META_DEFAULT_FOR`).
- **Sandbox runner and sandbox app.** `--agent-kind` and `--claude-binary`/`--agent-binary`; the recipe is sniffed from argv (`detect_recipe_from_argv`).

#### 4.4 How agents learn they are sandboxed or managed today

- **The tddy-tools relay.**
  - `TDDY_SOCKET` (host relay socket; set by `managed_launch.rs` and the backends).
  - `TDDY_SANDBOX_TOOL_IPC` (in-jail and host-agent IPC; `tddy-sandbox::scratch_runner_env`, `host_agent.rs:88`).
  - `TDDY_REMOTE_{DAEMON_URL,DAEMON_SOCKET,SESSION_ID,SESSION_TOKEN,DAEMON_INSTANCE_ID,LIVEKIT_URL,LIVEKIT_ROOM,LIVEKIT_TOKEN,SERVER_IDENTITY}` for remote, split and coding-agent tool servers (`RemoteToolEnv`, `SESSION_TOKEN_ENV`).
  - `TDDY_CONVERSATION_ID`, `TDDY_SESSION_DIR`, `TDDY_REPO_DIR`.
  - `TDDY_SUBAGENT`, `TDDY_SUBAGENTS_JSON`, `TDDY_SUBAGENT_REPLACES`, `TDDY_SUBAGENT_ROSTER_STATIC`, `TDDY_SUBAGENT_CODEBASE_ACCESS`, `TDDY_LSP_TOOLS`, `TDDY_SEMANTIC_INDEX_DB`, `TDDY_INDEX_SOCKET`.
  - `TDDY_TOOLS_LOG_FILE`, `TDDY_TOOLS_ACCOUNTING_FILE`, `TDDY_HOOK_TOKEN`, `TDDY_OS_USER`.
- **Jail environment.**
  - `TDDY_SANDBOX_EGRESS_DIR`, `TDDY_SANDBOX_SESSION_ID`, `HOME` (scratch or persistent home), `TMPDIR` (scratch).
  - `TDDY_EGRESS_SHIM` and `HTTPS_PROXY`/`HTTP_PROXY` point at the loopback egress shim (`runner.rs:1281-1286`); `TDDY_EGRESS_PROBE_{HOST,PORT,URL,TARGET}`.
  - `TDDY_SECRET_*` names files that the runner turns into env.
  - Per agent: `CLAUDE_CODE_TMPDIR`/`CLAUDE_TMPDIR`; `CURSOR_TMPDIR`/`CURSOR_SKIP_KEYCHAIN=1`/`CI=true`.
- **The tool surface is injected per agent, through MCP config plus permission flags.**
  - Claude: `--mcp-config` file, `--allowedTools mcp__tddy-tools__*`, `--disallowedTools` natives, `--permission-prompt-tool mcp__tddy-tools__approval_prompt`, `--strict-mcp-config --setting-sources user` (host agent).
  - Cursor: `.cursor/mcp.json` plus `--approve-mcps --trust`.
  - Codex: inline `-c mcp_servers.*`.
  - Oz: `--mcp`.
  - ACP agents: `session/new mcp_servers`.
  - claude-acp withdrawal: `_meta.claudeCode.options.disallowedTools`.
  - opencode withdrawal: `OPENCODE_CONFIG_CONTENT {"tools":{…:false}}`.
- **Files.**
  - The context dir carries `MANAGED_CODEBASE_PREAMBLE` at the top of `CLAUDE.md`/`AGENTS.md`.
  - `.claude/settings.local.json` hooks and `.cursor/hooks.json`.
  - `.cursor/rules/tddy-managed-workflow.mdc`.
  - `--append-system-prompt-file` (managed recipe, notify guidance, project-home prompt).
- **Code paths that branch on sandboxing.**
  - `req.sandbox` branches in `svc_start_session_core.rs:393,426`.
  - `require_provider_route` for jailed opencode and muse.
  - `allow_oauth_inbound` only when unrouted claude-cli or cursor-cli.
  - `CLAUDE_CODE_OAUTH_TOKEN` is injected only when unrouted claude-cli.
  - `refuse_unless_confinable` in managed sessions.
  - `refuse_unservable_sandboxed_session` (Cursor cannot use sandboxed codebase mode).
  - Split sessions are claude-cli only.

#### 4.5 Inventory: agent by concern by location

L means launch-time config (declarable in YAML). R means runtime protocol behaviour (stream, ACP, hooks, scraping).

| Agent | Concern | L/R | Location |
|---|---|---|---|
| **Claude (headless `claude -p`)** | binary `claude` | L | backend/claude.rs:263; daemon-kernel agent_cli.rs:114 (resolver) |
| | argv `-p … --output-format stream-json --verbose`, permission-mode, session-id/resume, append-system-prompt(-file), allowedTools, permission-prompt-tool, mcp-config | L | backend/claude.rs:69-139; headless_driver/argv.rs `claude()` |
| | goal to permission mapping (plan, acceptEdits) | L | claude.rs:192; workflow hints.rs |
| | MCP config temp file (`mcpServers.tddy-tools`) | L | claude.rs:166 |
| | env `TDDY_SOCKET`, `TDDY_REPO_DIR`, `TDDY_SESSION_DIR`, `TDDY_REMOTE_*`, PATH | L | claude.rs:405-424 |
| | stream-json parsing, session_id, AskUserQuestion | R | stream/claude.rs; stream/mod.rs; headless_driver/stream_json.rs |
| | exit 1 tolerated when output is structured | R | claude.rs:586; recipes set the hint |
| | transcript token usage `~/.claude/projects` | R | backend/claude/transcript_usage.rs; mod.rs:256; presenter/usage_watcher.rs:155 |
| | models (aliases plus pinned), default `opus` | L | backend/mod.rs:637-741 |
| | provider route `ANTHROPIC_*`, `CLAUDE_CODE_SUBAGENT_MODEL`, `ANTHROPIC_AUTH_TOKEN`, `--model` | L | provider_route.rs:364 |
| | context globs `CLAUDE.md`, `.claude/**`, `.mcp.json` | L | context_globs.rs:37 |
| | headless coding-subagent MCP file plus approval tool | L | coding_agent_launcher.rs:457 |
| | tool vocabulary in recipes (Read, Edit, Bash(...), ExitPlanMode, …) | L | workflow-recipes/permissions.rs |
| **Claude CLI (`claude-cli` PTY)** | registry descriptor (binary env `TDDY_CLAUDE_BINARY`, argv, models) | L | cli_harness.rs:78 |
| | base argv `--model --session-id/--resume --permission-mode/--dangerously-skip-permissions [prompt]` | L | claude_argv.rs; cli-sessions argv.rs |
| | daemon.yaml `claude_cli:` (binary, tools path, daemon_url, home, fullscreen, answer_wait) | L | daemon-kernel config/agent_cli.rs:148 |
| | binary resolution (`~/.local/bin`, skip `.superset/bin`, canonicalize) | L | agent_cli.rs:114 |
| | renderer env `CLAUDE_CODE_NO_FLICKER=1` | L | agent_cli.rs:205 |
| | hooks `.claude/settings.local.json` (8 events, await-answer, timeout) | L (file) / R (hook protocol) | claude_hooks.rs; session-split service_util.rs:304; agent-launch claude_cli_spawn_steps.rs:168 |
| | hook stdin parsing, PermissionRequest decision JSON, status mapping | R | tddy-tools session_hook.rs; changeset session_activity.rs:102 |
| | approval_prompt MCP tool (permission-prompt-tool format) | R | tddy-tools server.rs:65 |
| | sandbox: persistent home seeding (`.credentials.json`, local install mirror) | L | sandbox-recipes claude_cli.rs:20-116; daemon-sandbox sandbox_session.rs:837; duplicate in sandbox-app spawn.rs:173-204 |
| | sandbox: credential copies, policy, env overlay `CLAUDE_CODE_TMPDIR`/`CLAUDE_TMPDIR` | L | claude_cli.rs:119-160; plan.rs |
| | sandbox: `CLAUDE_CODE_OAUTH_TOKEN` secret when unrouted; OAuth inbound egress | L | plan.rs:285, `build_runner_plan` |
| | sandbox: in-jail MCP allow/disallow lists, native aliases, subagent tools, SemanticSearch | L | claude_cli.rs:164-405; runner.rs:1512-1615 |
| | sandbox runner flags `--claude-binary --permission-mode --dangerously-skip-permissions --resume --claude-arg` | L | runner.rs:286-340; agent-launch svc_start_sandboxed_claude_cli_session.rs:353-390; svc_relaunch_sandboxed_runner.rs:163 |
| | host-agent (sandboxed codebase) `--strict-mcp-config --setting-sources user`, native disallow | L | sandbox-app host_agent.rs:126; recipes `build_host_agent_disallowlist` |
| | split sessions are claude-cli only; native tool disallow | L | session-split split_session.rs:32; agent_argv.rs |
| | project-home session is claude-cli only | L | agent-launch project_home_launch.rs |
| | branch policy (refuse a blank branch name) | L | worktree-service branch_intent.rs:65 |
| | status, notify and question gates | R | session-activity hook_reports.rs:175, question_answers.rs:69, operator_notifications.rs:39; daemon-rpc notify.rs:26 |
| | requires a model | L | cli_harness.rs `cli_harness_requires_model`; claude_cli_spawn.rs |
| **Claude ACP (`claude-acp`)** | `bunx claude-agent-acp` or `TDDY_CLAUDE_AGENT_ACP_CLI` | L | backend/acp.rs:38; coding_agent_launcher.rs:67,283,737 |
| | route as env only | L | acp.rs; routed.rs |
| | native-tool withdrawal via `_meta.claudeCode` | L | coding-subagent confinement.rs; session-agents coding_agent.rs:203 |
| | ACP protocol (session/new, load, permissions) | R | acp_driver/* |
| | skips the tddy-tools check | L | tddy-coder run.rs:184 |
| **Cursor (`cursor` headless)** | binary `agent`, `TDDY_CURSOR_AGENT`, `--cursor-agent-path` | L | backend/cursor.rs:57; tddy-coder run.rs:2505; agent_cli.rs:337 |
| | argv `[--plan] [--resume] -p --model --output-format stream-json --stream-partial-output --force --trust --approve-mcps` (headless driver omits `--force`) | L | cursor.rs:176; headless_driver/argv.rs `cursor()` |
| | system prompt prepended to the prompt | L | cursor.rs:228 |
| | `.cursor/mcp.json` in cwd; in the coding-subagent path it is excluded from git and an existing one is refused | L | cursor.rs:208; coding_agent_launcher.rs:670-733 |
| | `agent --list-models` parsing | R | cursor.rs:111-170; daemon-rpc catalog/ports.rs:112, agent_models.rs:42 |
| | stream parsing | R | stream/cursor.rs |
| | route refused | L | provider_route.rs:235; routed.rs |
| | context globs (Claude set plus `.cursor/**`) | L | context_globs.rs:45 |
| | permission policy "read-only, cannot ask" | L | coding-subagent transports.rs |
| | activity source "cursor-cli" | L | tddy-coder run.rs:310 |
| **Cursor CLI (`cursor-cli` PTY)** | descriptor argv `--resume <chat> --model prompt`, curated models | L | cli_harness.rs:89,173; backend/mod.rs:745 |
| | daemon.yaml `cursor_cli:` | L | agent_cli.rs:248 |
| | chat id minted and persisted (`cursor_chat_id`) | L/R | cursor_cli_spawn.rs; changeset session_metadata.rs:42 |
| | `.cursor/hooks.json` (5 camelCase events), no daemon socket (TODO) | L/R | cursor_hooks.rs; agent-launch chat.rs |
| | `.cursor/rules/tddy-managed-workflow.mdc` | L | cursor_cli_spawn.rs:328; svc_start_sandboxed_cursor_cli_session.rs:250 |
| | sandbox: install-tree reads, node+index.js argv rewrite, local install mirror, creds `~/.cursor/{auth,cli-config,mcp}.json`, `CURSOR_TMPDIR`/`CURSOR_SKIP_KEYCHAIN`/`CI` | L | sandbox-recipes cursor_cli.rs; daemon-sandbox sandbox_session.rs:527,864; runner.rs:1660 |
| | runner `--agent-kind cursor --agent-binary --agent-arg` | L | svc_start_sandboxed_cursor_cli_session.rs:289-311 |
| | branch policy (generate a name); worktree not removed on delete | L | branch_intent.rs:74; session_deletion.rs:47 |
| | cannot use sandboxed codebase mode | L | sandbox-app main.rs:1019 |
| **Codex (`codex exec`)** | binary `codex`, `TDDY_CODEX_CLI`, `--codex-cli-path` | L | backend/codex.rs:50; tddy-coder codex_binaries.rs |
| | argv `exec -C -s read-only/workspace-write --json [resume] -m <id> <prompt>` | L | codex.rs:422; headless_driver/argv.rs `codex()` |
| | MCP inline `-c mcp_servers.<n>.*` | L | headless_driver/argv.rs `codex_mcp_server_overrides`; coding_agent_launcher.rs:487 |
| | thread id file `codex_thread_id` | R | codex.rs:29 |
| | JSONL parsing (thread.started, item.completed, turn.failed) | R | stream/codex.rs; headless_driver/codex_jsonl.rs |
| | OAuth: `codex login`, `BROWSER`=tddy-coder exe, `TDDY_CODEX_OAUTH_OUT`, URL scrape, loopback 1455 relay over LiveKit | R | codex.rs:79; tddy-coder-cli main.rs:9; daemon-auth codex_oauth_relay.rs; daemon-livekit oauth_loopback_tunnel.rs and codex_oauth_participant_metadata.rs; service codex_oauth_scan.rs and codex_oauth_validate.rs |
| | route `-c model_provider…`, `TDDY_ROUTE_API_KEY` | L | provider_route/codex.rs |
| | context globs `.codex/**`; permission "workspace-write" | L | context_globs.rs:53; transports.rs |
| **Codex ACP (`codex-acp`)** | binary `codex-acp` or `TDDY_CODEX_ACP_CLI`, beside the resolved `codex` | L | codex_acp.rs:27,128; codex_binaries.rs; coding_agent_launcher.rs:62 |
| | route args rewritten to `-c model=` | L | routed.rs `codex_acp_route_args` |
| | auth error leads to login and one retry | R | codex_acp.rs:63; codex_acp/oauth.rs |
| | cannot withdraw native tools, so refused in managed sessions | L | confinement.rs |
| **OpenCode ACP (`opencode`)** | `opencode acp`, `TDDY_OPENCODE_CLI` | L | opencode_acp.rs:158; coding_agent_launcher.rs:292 |
| | route: generated `opencode.json` via `OPENCODE_CONFIG` (backend) or `OPENCODE_CONFIG_CONTENT` (coding-subagent), key `TDDY_OPENCODE_PROVIDER_KEY` | L | provider_route.rs:320-492; opencode_acp.rs; coding_agent_launcher.rs:305-344 |
| | native-tool withdrawal `{"tools":{bash:false,…}}` | L | confinement.rs |
| | default model `fireworks/kimi-k2` | L | backend/mod.rs:645 |
| | context globs `opencode.json`, `.opencode/**` | L | context_globs.rs:55 |
| **OpenCode CLI (`opencode-cli` PTY)** | descriptor argv `--continue` / `--model --prompt`; models via `opencode models`; `route_config_env=OPENCODE_CONFIG` | L | cli_harness.rs:100,194 |
| | binary via `cli_harnesses.opencode-cli` | L | agent_cli.rs `cli_harness_binary`; opencode_cli_spawn.rs:81 |
| | sandbox: Claude policy plus exec reads; route required; route file materialised | L | sandbox-recipes opencode_cli.rs, route_files.rs; cli_harness_jail.rs:91 |
| **Muse (`muse-cli`)** | descriptor (`muse`, `TDDY_MUSE_CLI`, models, argv), route `META_API_KEY`, Meta only | L | cli_harness_muse.rs; provider_route.rs:229,347 |
| | sandbox: no-JIT policy; route required | L | sandbox-recipes muse_cli.rs |
| **Warp (`warp-cli`)** | descriptor (`warp`, `TDDY_WARP_CLI`, no model) | L | cli_harness_warp.rs |
| | resume token scraped from PTY exit output and persisted as `harness_resume_token` | R | cli_harness_warp.rs `resume_token_from_output`; agent-launch cli_resume_token.rs |
| **Oz (headless Warp)** | `oz agent run -p --output-format ndjson -C --conversation --model --mcp --profile` | L | headless_driver/argv.rs `oz()`; coding_agent_launcher.rs:87,471 |
| | NDJSON parsing (conversation_started, tool_call) | R | headless_driver/oz_ndjson.rs |
| **Stub / Mock** | in-memory executor, magic prompts, no route, cache-ineligible (mock) | R | backend/stub.rs, mock.rs; tddy-coder run.rs:184; tddy-acp-stub (ACP stand-in) |

#### 4.6 Cross-cutting observations

- **Duplicated agent tables.**
  - The agent-id to backend construction exists twice: `tddy-coder/src/run.rs:create_backend` and `model_catalog.rs:build_backend`.
  - Claude native-tool lists exist three times: `coding-subagent confinement.rs CLAUDE_NATIVE_TOOLS`, `session-split split_session.rs NATIVE_FILESYSTEM_TOOLS`, and `sandbox-recipes NATIVE_EXEC_TOOL_NAMES` plus `native_aliases`.
  - Credential seeding exists twice: `sandbox-recipes` and `sandbox-app/spawn.rs`.
  - The `mcpServers.tddy-tools` JSON is written in at least 5 places: `claude.rs`, `cursor.rs`, `sandbox-recipes write_claude_mcp_config`, `sandbox-recipes write_cursor_mcp_config`, and `coding_agent_launcher mcp_servers_wrapped`.
  - Approval-tool constants exist three times: `claude.rs`, `PERMISSION_PROMPT_TOOL`, `APPROVAL_PROMPT_TOOL`.
- **Two naming vocabularies.** Agent ids (claude, cursor, codex, opencode, muse, warp) and session types (claude-cli, cursor-cli, …) are bridged by `CliHarness.context_agent`, `Harness::id`, `listing_harness_session` and `context_agent_for_session_type`.
- **Declarative precursors already exist.** `CliHarness` descriptors, `HarnessRoute` (pure data: env, secret_env, files, args, serialised as `provider-route.json`), `HarnessProfile` YAML, and `SandboxRecipe` plans. The parts that are not declarative are:
  - fn-pointer argv builders;
  - stream parsers (claude, cursor, codex, oz);
  - hook protocols (Claude and Cursor hooks JSON, PermissionRequest decision output);
  - Codex OAuth capture and relay;
  - Warp resume-token scraping;
  - Cursor `--list-models` and `opencode models` parsing;
  - ACP `_meta` and inline-config withdrawal;
  - the per-recipe sandbox reads, policy, credential copies and env overlays.
- **Agent-specific fields in generic types.**
  - `GoalHints.claude_nonzero_exit_ok_if_structured_response`, `agent_cli_plan_mode`.
  - `InvokeRequest.session` semantics.
  - `SessionMetadata.cursor_chat_id`, `harness_resume_token`.
  - StartSessionRequest `permission_mode`, `dangerously_skip_permissions`, `claude_args`.
  - Runner args `--claude-binary`, `--claude-arg`.
  - The terminal named `"claude"`, the action kind `"claude-cli"`, the tddy-tools `pty_relay` default `claude-cli`/`opus`.
- **Stale or inconsistent points.**
  - The `opencode_cli_spawn.rs` doc says "never sandboxed", but a sandboxed opencode and muse path exists.
  - Cursor `--force` is passed by `CursorBackend` and pinned absent in the headless driver.
  - `build_sandbox_runner_env` always adds the Claude overlay, whatever the agent.
  - Cursor hooks get no daemon socket (TODO in `agent-launch/src/chat.rs:30`).
  - `TDDY_MUSE_PROVIDER_KEY` appears once, while `MUSE_ROUTE_KEY_ENV` is `META_API_KEY`.
- **Distinct `TDDY_*` env vars in the launch crates.** `TDDY_REPO_DIR`, `TDDY_SOCKET`, `TDDY_SUBAGENTS_JSON`, `TDDY_SUBAGENT`, `TDDY_SESSION_DIR`, `TDDY_REMOTE_{SESSION_ID,SESSION_TOKEN,DAEMON_URL,DAEMON_SOCKET,SERVER_IDENTITY,DAEMON_INSTANCE_ID,LIVEKIT_URL,LIVEKIT_TOKEN,LIVEKIT_ROOM}`, `TDDY_TOOLS_LOG_FILE`, `TDDY_SANDBOX_TOOL_IPC`, `TDDY_SANDBOX_EGRESS_DIR`, `TDDY_OPENCODE_PROVIDER_KEY`, `TDDY_SUBAGENT_REPLACES`, `TDDY_EGRESS_PROBE_{URL,PORT,HOST,TARGET}`, `TDDY_CODEX_ACP_CLI`, `TDDY_TOOLS_ACCOUNTING_FILE`, `TDDY_SANDBOX_SESSION_ID`, `TDDY_QUIET`, `TDDY_CONVERSATION_ID`, `TDDY_CODEX_CLI`, `TDDY_SUBAGENT_ROSTER_STATIC`, `TDDY_OPENCODE_CLI`, `TDDY_LSP_TOOLS`, `TDDY_CURSOR_AGENT`, `TDDY_CODEX_OAUTH_OUT`, `TDDY_WORKFLOW_SESSION_ID`, `TDDY_WARP_CLI`, `TDDY_SUBAGENT_CODEBASE_ACCESS`, `TDDY_SESSION_BASE`, `TDDY_SEMANTIC_INDEX_DB`, `TDDY_SECRET_*`, `TDDY_SANDBOX_{CURSOR,CLAUDE,CODEBASE}_HOME`, `TDDY_SANDBOX_BACKEND`, `TDDY_ROUTE_API_KEY`, `TDDY_MUSE_PROVIDER_KEY`, `TDDY_MUSE_CLI`, `TDDY_INDEX_SOCKET`, `TDDY_EGRESS_SHIM`, `TDDY_DAEMON_INSTANCE_ID`, `TDDY_CLAUDE_BINARY`, `TDDY_CLAUDE_AGENT_ACP_CLI`. Also `TDDY_HOOK_TOKEN`/`TDDY_OS_USER` (tddy-service), `TDDY_AGENT_DRIVEN` (presenter), `TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE` (daemon).
- **Docs to cross-reference.** `docs/ft/daemon/{harness-profiles, session-agent-roster, claude-cli-session, cursor-cli-session, opencode-cli-session, muse-cli-session, warp-cli-session, claude-cli-permission-mode, agent-context-sync, agent-session-status}.md`, `tddy-agent-backend/docs/architecture.md`, `tddy-coding-subagent/docs/architecture.md`, `tddy-sandbox-recipes/docs/changesets/*`.
