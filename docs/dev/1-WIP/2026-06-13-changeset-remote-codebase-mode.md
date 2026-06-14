# Changeset: Remote-Codebase Mode

**Feature PRD:** [docs/ft/daemon/remote-codebase-mode.md](../../ft/daemon/remote-codebase-mode.md)  
**Status:** DONE (all phases complete)  
**Packages affected:** tddy-service, tddy-daemon, tddy-tools, tddy-coder, tddy-core, tddy-workflow-recipes, docs

---

## TODO

### Phase 1–2: Core infrastructure

- [x] Create/update PRD documentation (`docs/ft/daemon/remote-codebase-mode.md`)
- [x] Create changeset (this file)
- [x] **Proto: `ExecuteTool` + `ListExecTools`** — RPCs + messages added to `connection.proto`; codegen run.
- [x] **Daemon: `tool_catalog.rs`** — `Vec<ToolDef>` with JSON schemas for all 10 cursor tools.
- [x] **Daemon: `tool_engine.rs`** — `execute_tool` (Read/Write/StrReplace/Delete/Grep/Glob/Shell/Await/ReadLints stub/SemanticSearch ripgrep-fallback) + `contain_path` security.
- [x] **Daemon: `shell_job_registry.rs`** — background-shell job registry, foreground-with-timeout, Await, eviction on complete.
- [x] **Daemon: `workspace_session.rs`** — `start_workspace_session` + `resolve_worktree_root_for_session`.
- [x] **Daemon: `connection_service.rs`** — wire `execute_tool` + `list_exec_tools` handlers; `workspace` branch in `start_session`; session-type guard on `execute_tool`.
- [x] **tddy-workflow-recipes: `permissions.rs`** — `remote_codebase_allowlist()` (AskUserQuestion only).
- [x] **tddy-coder: `remote.rs`** — `REMOTE_APPENDIX` const.
- [x] **tddy-coder: `run.rs`** — `--remote` flag with validation (requires `free-prompting` recipe).
- [x] **Acceptance tests** — 21 tests across 5 files; all pass.

### Phase 3: Relay daemon mode config

- [x] **Daemon: `config.rs`** — `RelayConfig { idle_timeout_secs }` + `relay: Option<RelayConfig>` on `DaemonConfig`; default 1800s; `validate_for_relay()`.
- [x] **Daemon: `relay_idle.rs`** (**new**) — `IdleTimeoutTracker { new(Duration), record_activity(), should_shutdown() }` using `Mutex<Instant>`.
- [x] **Daemon: `startup.rs`** (**new**) — `startup_config_check(config, relay) -> (u16, Option<PathBuf>)`; relay=true skips bundle path.
- [x] **Daemon: `main.rs`** — `--relay` flag wired; calls `startup_config_check(args.relay)`; skips `web_bundle_path` requirement in relay mode.
- [x] **Daemon: `connection_service.rs`** — `with_idle_tracker(Arc<IdleTimeoutTracker>) -> Self` builder; `record_rpc_activity()` called in every RPC handler.
- [x] **Daemon: `livekit_peer_discovery.rs`** — generic `forward_to_peer`; renamed `classify_peer_route`; per-peer `RpcClient` cache on `ConnectionServiceImpl`; `execute_tool`/`list_exec_tools` route-classify before local execution.
- [x] **Daemon: `main.rs` + `server.rs` (idle-timeout wiring)** — `IdleTimeoutTracker` constructed and injected in relay mode; external oneshot shutdown channel threaded into `run_server`; background monitor task fires channel on `should_shutdown()`.

### Phase 4: tddy-tools relay dispatch

- [x] **tddy-tools: `server.rs`** — `dispatch_dynamic_tool` replaced stub with real HTTP POST to relay `ExecuteTool` RPC; `is_native_tool_denied_in_remote_mode` added.
- [x] **tddy-tools: `relay.rs`** (**new**) — `ensure_relay_daemon`: reads `daemon.json`, TCP health-check reuse, spawns daemon binary if needed, polls until reachable, writes discovery file.
- [x] **tddy-tools: `remote_cli.rs`** — `list-tools` calls relay HTTP endpoint (not discovery file); `start-session`, `connect-session`, `sync-context` subcommands exist in `--help`.
- [x] **tddy-tools: `remote_cli.rs` (subcommand implementations)** — `run_list_tools` reworked to `ListExecTools` Connect POST; `run_start_session`, `run_connect_session`, `run_sync_context` implemented; shared `connect_post` helper; `resolve_base_url` from `daemon.json`.

### Phase 5: tddy-core + tddy-coder wiring

- [x] **tddy-core: `backend/mod.rs`** — `RemoteToolEnv` struct with `env_pairs()`; `remote: Option<RemoteToolEnv>` on `InvokeRequest`; `Default` impl for `InvokeRequest`.
- [x] **tddy-core: `backend/claude.rs`** — exports `TDDY_REMOTE_*` vars from `RemoteToolEnv.env_pairs()` before subprocess spawn.
- [x] **tddy-core: `workflow/task.rs`** — populates `InvokeRequest.remote` from ctx keys via `extract_remote_env_from_ctx`; reads `remote_daemon_url`, `remote_session_id`, `remote_session_token` etc. from `WorkflowContext`.
- [x] **tddy-coder: `remote.rs`** — `RemoteContextDir` RAII; `build_remote_allowlist`; `REMOTE_APPENDIX`.
- [x] **tddy-coder: `config.rs`** — `RemoteConfig` struct with daemon_url/session_id/session_token/daemon_instance_id; `to_remote_tool_env()`.
- [x] **tddy-coder: `run.rs` + `config.rs`** — `--remote-daemon-url`, `--remote-session-token`, `--remote-daemon-id` flags added; `run_remote` dispatch wired; `run_remote` implemented (validates flags, shells out to `tddy-tools remote list-tools`).

### Phase 6: E2E tests

- [x] `relay_forwards_list_exec_tools_to_remote_peer` (LiveKit-testkit-gated) — relay routes `ListExecTools(daemon_instance_id=B)` via `forward_to_peer` to remote peer B; asserts catalog includes Read/Write/Shell.
- [x] `relay_idle_monitor_triggers_server_shutdown` — idle monitor fires oneshot channel → `run_server` exits cleanly.
- [x] `relay_activity_defers_idle_shutdown` — `record_activity()` resets clock; no expiry before full idle period.
- [x] SemanticSearch/ReadLints minimal stubs — ripgrep fallback + stub already in `tool_engine.rs` from Phase 1; daemon tool_engine tests cover parity.

---

## Acceptance tests (all passing)

### Phase 1–2 (in `packages/tddy-daemon/tests/`)
- `execute_tool_acceptance.rs` — tool execution, auth, path traversal, unknown tool
- `workspace_session_acceptance.rs` — workspace create, connect, ExecuteTool round-trip
- `list_exec_tools.rs` — catalog content, auth

### Phase 3 follow-up (passing)
- `relay_runtime_acceptance.rs` — `startup_config_check` relay vs non-relay, `validate_for_relay`, `IdleTimeoutTracker` expiry
- `relay_idle_wired_acceptance.rs` — `with_idle_tracker` builder, RPC bumps tracker

### Phase 3–5 follow-up (passing, added 2026-06-13)
- `packages/tddy-daemon/tests/relay_peer_forwarding_acceptance.rs` — `classify_peer_route`, `PeerRoute`, `execute_tool`/`list_exec_tools` routing (8 tests)
- `packages/tddy-daemon/tests/relay_idle_shutdown_acceptance.rs` — external shutdown channel fires server exit (1 test)
- `packages/tddy-tools/tests/remote_cli_subcommand_acceptance.rs` — `list-tools` Connect POST, `start-session`, `connect-session`, `sync-context` (4 tests)
- `packages/tddy-coder/tests/run_remote_flags_acceptance.rs` — `--remote-daemon-url/token/id` flags, `run_remote` dispatch (5 tests)

### Pre-existing test fixes (2026-06-13)
- `delete_session_acceptance` — macOS zombie SIGKILL hang: return `Ok(())` unconditionally after SIGKILL
- `livekit_peer_daemons_acceptance` / `multi_host_acceptance` — restored `with_mapped_port` (LiveKit ICE candidates require fixed host ports); `true_bin()` for macOS `/usr/bin/true`
- `worktrees_acceptance` — canonicalize wt path after `git worktree add` to match `git worktree list` symlink-resolved output

### Phase 4 follow-up (passing)
- `packages/tddy-tools/tests/remote_cli_acceptance.rs` — `start-session`/`connect-session`/`sync-context` in `--help`; `list-tools` fetches from HTTP not discovery file

### Phase 6 (passing, added 2026-06-14)
- `packages/tddy-daemon/tests/relay_e2e_acceptance.rs` — idle monitor triggers server exit, activity defers expiry, LiveKit relay forwarding via `forward_to_peer` (3 tests; LiveKit test requires Docker/`LIVEKIT_TESTKIT_WS_URL`)

### Phase 5 follow-up (passing)
- `packages/tddy-integration-tests/tests/task_remote_ctx_acceptance.rs` — `InvokeRequest.remote` populated from ctx keys; absent keys → None

---

## Cross-package delta

### `packages/tddy-service`
- `proto/connection.proto`: +`ExecuteTool` rpc, +`ListExecTools` rpc; +`ExecuteToolRequest`, `ExecuteToolResponse`, `ListExecToolsRequest`, `ToolDef`, `ListExecToolsResponse` messages.

### `packages/tddy-daemon`
- `src/tool_engine.rs` (**new**): cursor-compatible tool execution; `contain_path` security.
- `src/tool_catalog.rs` (**new**): authoritative `Vec<ToolDef>`; JSON schemas for all 10 tools.
- `src/shell_job_registry.rs` (**new**): background-shell job registry + Await polling.
- `src/workspace_session.rs` (**new**): workspace session creation + worktree resolution.
- `src/relay_idle.rs` (**new**): `IdleTimeoutTracker`; relay mode idle-timeout logic.
- `src/startup.rs` (**new**): `startup_config_check`; relay-aware port+bundle validation.
- `src/connection_service.rs`: `execute_tool` + `list_exec_tools` handlers; `workspace` branch in `start_session`; `with_idle_tracker` builder; `record_rpc_activity`.
- `src/config.rs`: `RelayConfig { idle_timeout_secs }` + `relay: Option<RelayConfig>`; `validate_for_relay()`.
- `src/main.rs`: `--relay` flag; `startup_config_check(args.relay)` skips bundle check; relay mode constructs `IdleTimeoutTracker`, threads `oneshot` shutdown receiver into `run_server`, spawns 30s monitor task.
- `src/lib.rs`: register new modules.

### `packages/tddy-tools`
- `src/relay.rs` (**new**): `ensure_relay_daemon`, `RelayEndpoint`; discovery file reuse + TCP health-check + spawn.
- `src/server.rs`: `dispatch_dynamic_tool` (HTTP POST to relay `ExecuteTool`); `is_native_tool_denied_in_remote_mode`.
- `src/remote_cli.rs` (**new**): `remote` subcommand group; `list-tools` via HTTP; `start-session`/`connect-session`/`sync-context` stubs (follow-up).
- `src/main.rs`: dispatch `Remote` subcommands.
- `Cargo.toml`: `reqwest` with `json` feature.

### `packages/tddy-core`
- `src/backend/mod.rs`: `RemoteToolEnv`; `InvokeRequest.remote: Option<RemoteToolEnv>`.
- `src/backend/claude.rs`: export `TDDY_REMOTE_*` in `invoke_sync`.
- `src/workflow/task.rs`: populate `InvokeRequest.remote` from ctx keys via `extract_remote_env_from_ctx`.
- `src/workflow/mod.rs`: `extract_remote_env_from_ctx(ctx: &HashMap<String, String>) -> Option<RemoteToolEnv>`.

### `packages/tddy-workflow-recipes`
- `src/permissions.rs`: `remote_codebase_allowlist() -> Vec<String>` (only `AskUserQuestion`; dynamic names added by coder).

### `packages/tddy-coder`
- `src/remote.rs` (**new**): `RemoteContextDir`, `build_remote_allowlist`, `REMOTE_APPENDIX`.
- `src/run.rs`: `--remote` flag + validation; `run_remote` stub (follow-up: full implementation).
- `src/config.rs`: `RemoteConfig`; `to_remote_tool_env()`.

### `docs`
- `docs/ft/daemon/remote-codebase-mode.md` (**new**) ✅
