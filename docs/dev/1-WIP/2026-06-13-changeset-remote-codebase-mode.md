# Changeset: Remote-Codebase Mode

**Feature PRD:** [docs/ft/daemon/remote-codebase-mode.md](../../ft/daemon/remote-codebase-mode.md)  
**Status:** WIP (green phase — Phase 1–2 implemented; Phases 3–6 pending)  
**Packages affected:** tddy-service, tddy-daemon, tddy-tools, tddy-coder, tddy-core, tddy-workflow-recipes, docs

---

## TODO

### Phase 1–2: Core infrastructure (implemented in this PR)

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

### Phase 3: Relay daemon mode (pending)

- [ ] **Daemon: `livekit_peer_discovery.rs`** — generic `forward_to_peer`; cached `RpcClient` per peer.
- [ ] **Daemon: `config.rs` + `main.rs` + `server.rs`** — `--relay` mode: optional `web_bundle_path`, `relay.idle_timeout_secs`, idle-timeout middleware, route-before-user-resolution.

### Phase 4: tddy-tools full relay client (pending)

- [ ] **tddy-tools: `remote_client.rs`** — extract `connectrpc_post` + `exchange_stub_session_token`; add `remote` cargo feature.
- [ ] **tddy-tools: `relay.rs`** — `ensure_relay_daemon` (discovery file, flock, lazy-spawn, health-check). Currently `remote_cli.rs` has a stub.
- [ ] **tddy-tools: `server.rs`** — FIXME: `dispatch_dynamic_tool` is a stub that always returns an error; implement actual relay forwarding via `ExecuteTool` RPC. Drop `#[tool_router]`; implement `list_tools`/`call_tool` by hand; hard-deny native tools when `TDDY_REMOTE_SESSION_ID` set.
- [ ] **tddy-tools: `cli.rs` + `main.rs`** — full `remote` subcommand group: `start-session`, `connect-session`, `resume-session`, `sync-context`. (`list-tools` stub is present.)

### Phase 5: tddy-core + tddy-coder wiring (pending)

- [ ] **tddy-core: `backend/mod.rs`** — `RemoteToolEnv` struct; add `remote: Option<RemoteToolEnv>` to `InvokeRequest`.
- [ ] **tddy-core: `backend/claude.rs`** — export `TDDY_REMOTE_*` env vars from `RemoteToolEnv`.
- [ ] **tddy-core: `workflow/task.rs`** — populate `InvokeRequest.remote` from ctx keys.
- [ ] **tddy-coder: `remote.rs`** — `RemoteContextDir` RAII; full `bootstrap_remote_session` (shells out to `tddy-tools remote …`); context sync.
- [ ] **tddy-coder: `run.rs`** — full `run_remote` implementation; ctx wiring; allowlist construction from `tddy-tools remote list-tools`.
- [ ] **tddy-coder: `config.rs`** — `RemoteConfig`; `merge_remote_*` fields.

### Phase 6: E2E tests (pending)

- [ ] `relay_daemon_forwards_execute_tool_to_remote_peer` (LiveKit-testkit-gated)
- [ ] `relay_daemon_lazy_starts_then_idle_times_out`
- [ ] SemanticSearch/ReadLints minimal stubs; full e2e parity tests.

---

## Acceptance tests

Fully-implemented failing tests to be written in the red phase (see below):

- `remote_workspace_session_round_trips_write_then_read`
- `tddy_tools_mcp_advertises_discovered_tools_no_hardcoded_list`
- `relay_daemon_forwards_execute_tool_to_remote_peer`
- `remote_mode_allowlist_built_from_discovery_excludes_native_tools`
- `remote_context_dir_is_read_only_with_appendix`
- `relay_daemon_lazy_starts_then_idle_times_out`

---

## Cross-package delta

### `packages/tddy-service`
- `proto/connection.proto`: +`ExecuteTool` rpc, +`ListExecTools` rpc; +`ExecuteToolRequest`, `ExecuteToolResponse`, `ListExecToolsRequest`, `ToolDef`, `ListExecToolsResponse` messages.

### `packages/tddy-daemon`
- `src/tool_engine.rs` (**new**): cursor-compatible tool execution; `contain_path` security.
- `src/tool_catalog.rs` (**new**): authoritative `Vec<ToolDef>`; JSON schemas for all 10 tools.
- `src/shell_job_registry.rs` (**new**): background-shell job registry + Await polling.
- `src/workspace_session.rs` (**new**): workspace session creation + worktree resolution.
- `src/connection_service.rs`: `execute_tool` + `list_exec_tools` handlers; `workspace` branch in `start_session`; route-before-user-resolution; cached peer `RpcClient`.
- `src/livekit_peer_discovery.rs`: `forward_to_peer(room_slot, peer_id, service, method, body)` (generalized from `forward_start_session_via_livekit`).
- `src/config.rs`: `RelayConfig { idle_timeout_secs }` + `relay: Option<RelayConfig>`.
- `src/main.rs`: `--relay` flag; skip `web_bundle_path` check in relay mode.
- `src/server.rs`: idle-timeout middleware (bumps `AtomicU64` last-activity on each RPC; background task triggers graceful shutdown).
- `src/lib.rs`: register new modules.
- `tests/execute_tool_acceptance.rs` (**new**)
- `tests/workspace_session_acceptance.rs` (**new**)
- `tests/list_exec_tools.rs` (**new**)
- `tests/relay_forwarding.rs` (**new**, LiveKit-testkit-gated)

### `packages/tddy-tools`
- `src/remote_client.rs` (**new**): `connectrpc_post`, `exchange_stub_session_token`, `execute_tool_via_relay`, `list_exec_tools_via_relay`.
- `src/relay.rs` (**new**): `ensure_relay_daemon`, `RelayEndpoint`, discovery file, flock, lazy-spawn.
- `src/server.rs`: drop `#[tool_router]`/`#[tool_handler]`; hand-implement `list_tools` + `call_tool`; hard-deny native tools when `TDDY_REMOTE_SESSION_ID` set.
- `src/cli.rs`: `Remote` subcommand enum + handlers.
- `src/main.rs`: dispatch `Remote` subcommands.
- `Cargo.toml`: `remote` feature (reqwest, prost, tddy-service); `livekit` depends on `remote`.

### `packages/tddy-core`
- `src/backend/mod.rs`: `RemoteToolEnv`; `InvokeRequest.remote: Option<RemoteToolEnv>`.
- `src/backend/claude.rs`: export `TDDY_REMOTE_*` in `invoke_sync`.
- `src/workflow/task.rs`: populate `InvokeRequest.remote` from ctx keys.

### `packages/tddy-workflow-recipes`
- `src/permissions.rs`: `remote_codebase_allowlist() -> Vec<String>` (only `AskUserQuestion`; dynamic names added by coder).

### `packages/tddy-coder`
- `src/remote.rs` (**new**): `RemoteContextDir`, `bootstrap_remote_session`, `sync_remote_context`, `REMOTE_APPENDIX`.
- `src/run.rs`: `--remote` + `--remote-daemon-id` + `--remote-daemon-url` + `--remote-session-token` args; `run_remote`; `validate_remote_cli`.
- `src/config.rs`: `RemoteConfig`; `merge_remote_*`; update `Args` literals.

### `docs`
- `docs/ft/daemon/remote-codebase-mode.md` (**new**) ✅
- `docs/ft/daemon/changelog.md`: add entry.
- `docs/dev/changesets.md`: prepend one-line bullet after wrap.
