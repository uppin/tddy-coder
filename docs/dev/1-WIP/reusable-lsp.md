# Changeset: reusable-lsp — a reusable language server exposed to agents via one MCP interface

**Date:** 2026-07-21
**Branch:** `special-tangerine`
**Packages:** `tddy-lsp` (new), `tddy-lsp-executor` (new), `tddy-core`, `tddy-tool-engine`, `tddy-tools`, `tddy-daemon`, `tddy-sandbox-app`
**Feature PRD:** [docs/ft/coder/reusable-lsp.md](../../ft/coder/reusable-lsp.md)
**Plan:** `plans/let-s-support-re-usable-lsp-wiggly-haven.md` (approved)

## Summary

Run a language server (rust-analyzer first, any LSP thereafter) as a long-running
`tddy-task`, reused across build targets and keyed by (workspace root, language). The
server is chosen by target type, gated by a Rust-only allow-list, owned by the daemon
(coder-fallback), and surfaced to agents through a single language-agnostic MCP tool set
(Diagnostics, Definition, References, Hover, Symbols). Mirrors the `BuildExecutor`
extension pattern (`tddy-core` trait + `OnceLock` registry; concrete impl in the binary).

**New dependency (approved):** `lsp-types` (types only — JSON-RPC framing hand-rolled
over `tddy-task` channels).

## Scope of this red phase

This changeset lands the **PRD, changeset, and the failing tests** (plan-red). The
`tddy-lsp` crate is scaffolded with its public API surface (`unimplemented!()` bodies)
so its tests compile and fail at runtime for the right reason (missing implementation).
Cross-crate wiring (daemon dispatch, sandbox env overlay, `ReadLints` upgrade) is
captured in the TODO and pinned by focused red tests; the deepest daemon e2e is marked
as a follow-up so the red phase stays bounded.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] `tddy-lsp` (new crate): scaffold `Cargo.toml` + `lib.rs`; add to workspace members
- [x] `tddy-lsp`: `allowlist.rs` — `Language`, `LaunchSpec`, `LspAllowList` (`rust_only()`), `language_for_target_type`
- [x] `tddy-lsp`: `error.rs` — `LspError` (incl. `LanguageNotAllowed`, `ServerNotFound`)
- [x] `tddy-lsp`: `protocol.rs` — Content-Length framing + JSON-RPC message parse/encode
- [x] `tddy-lsp`: `client.rs` — `LspClient` (initialize/did_open/diagnostics/definition/references/hover/symbols/workspace_symbols/shutdown); id↔oneshot correlation; publishDiagnostics cache
- [x] `tddy-lsp`: `server_body.rs` — `LspServerBody` impl `TaskBody` (streaming stdio bridge, handshake, cancel-vs-exit select, `register_child_pid`)
- [x] `tddy-lsp`: `registry.rs` — `LspKey`, `LspService`, `LspRegistry` (get_or_spawn, bind_target, reap_idle, respawn-if-terminal), `srcs_to_document_sources`, workspace-root detection
- [x] `tddy-lsp`: `tests/bin/fake_lsp.rs` — deterministic fake LSP server (+ hang mode)
- [~] `tddy-task`: lift `IdleTimeoutTracker` — **not needed for this slice**; the registry tracks idle inline with a per-key `Instant` (no `tddy-daemon` dependency introduced). Lift only if a second consumer wants it.
- [x] `tddy-core`: `toolcall/lsp.rs` — `LspExecutor` trait + `LspQuery` + `OnceLock` registry (`register_lsp_executor`/`lsp_executor`); `pub mod lsp;` + re-exports in `toolcall/mod.rs`
- [x] **`tddy-lsp-executor` (new crate):** `TddyLspExecutor` impls `tddy-core::LspExecutor` over `tddy-lsp` + `tddy-build` discovery (target id → `config.type` → `Language` → allow-list → `LspKey{workspace_root, language}`); `register(task_registry, allow, idle_timeout)`. Registered by the **daemon** and **sandbox-app** (the Path-B hosts that run `tddy-tool-engine`), NOT `tddy-coder`. (Corrects the original plan: `Lsp*` tools flow over the session-tool transport, not the `TDDY_SOCKET` listener that `BuildExecutor` uses.)
- [x] `tddy-tools`: `lsp_tools.rs` — `lsp_tool_catalog()` (5 language-agnostic defs) + `lsp_tools_enabled()` (`TDDY_LSP_TOOLS` gate)
- [x] `tddy-tools`: `server.rs` — merge `lsp_tool_catalog` via `dynamic_tool_router` in `PermissionServer::new()` behind the `TDDY_LSP_TOOLS` gate (dispatch reuses `dispatch_dynamic_tool` → session-tool transport; no per-tool handler needed)
- [x] `tddy-tool-engine`: `lib.rs` — intercept the five `Lsp*` names in `execute_tool_with_env` → `lsp_executor()` (single dispatch site serving all three host handlers: daemon HTTP, daemon sandbox IPC, sandbox-app)
- [x] `tddy-daemon`: register the executor at startup (`main.rs`, sharing the daemon `TaskRegistry`) + idle-reaper loop; `lsp_tools_env(worktree)` sets `TDDY_LSP_TOOLS` at the three sandboxed-session env sites (`connection_service.rs`). Dispatch is via `tddy-tool-engine`, so no `ExecuteTool` match edit was needed.
- [ ] `tddy-sandbox-app`: register the executor before spawn (`main.rs::run_macos`) + set `TDDY_LSP_TOOLS` in the jail env (`spawn.rs`) when `is_available(&params.repo)` — **in progress**
- [x] `tddy-tool-engine`: `tool_read_lints` routes to `LspExecutor::workspace_diagnostics` (a `workspace/diagnostic` pull, backed by `LspClient::workspace_diagnostics`) when a server is available for the repo, else the no-linter stub. Added `workspace_diagnostics` to the `LspExecutor` trait + all impls, `LspClient`, and the fake server.
- [ ] **Deferred:** full daemon e2e (`agent_gets_lsp_tools_and_finds_references_across_two_targets`). The end-to-end path is covered compositionally by the per-crate tests (executor availability, tool-engine dispatch, MCP gate/merge, client round-trips); a live daemon+jail+fake-server e2e is a heavy follow-up.
- [ ] **Follow-up (robustness):** `LspClient` currently ignores server→client requests (e.g. `client/registerCapability`, `window/workDoneProgress/create`). Real rust-analyzer tolerates the missing replies (logs a timeout) and the core ops work, but responding to them would make advanced features fully robust.

## Acceptance tests (headline behaviours — present for review)

- [x] `packages/tddy-lsp/tests/registry_reuse_test.rs` — `two_targets_in_one_workspace_reuse_a_single_language_server`, `different_workspace_roots_get_separate_language_servers`
- [x] `packages/tddy-lsp/tests/client_roundtrip_test.rs` — references/definition/hover/symbols/diagnostics round-trips, concurrent-id correlation
- [x] `packages/tddy-tools/src/lsp_tools.rs` (`#[cfg(test)]`) — `the_five_lsp_tools_are_absent_without_the_availability_gate`, `the_five_lsp_tools_are_present_behind_the_availability_gate`, `lsp_tool_names_are_language_agnostic`

## Unit / integration tests

- [x] `tddy-lsp/src/allowlist.rs` — `rust_binary_target_type_maps_to_the_rust_language`, `rust_library_target_type_maps_to_the_rust_language`, `an_unknown_target_type_maps_to_no_language`, `the_default_allow_list_permits_rust`
- [x] `tddy-lsp/tests/registry_reuse_test.rs` — `requesting_a_disallowed_language_returns_an_error_and_spawns_no_server`, `an_idle_language_server_is_torn_down_after_the_timeout`, `activity_keeps_a_language_server_alive_past_the_idle_timeout`, `a_crashed_language_server_is_respawned_on_the_next_request`
- [x] `tddy-lsp/tests/server_body_test.rs` — `a_language_server_task_stays_running_until_it_is_cancelled`, `an_unresponsive_language_server_is_killed_after_the_grace_period`
- [x] `tddy-lsp/tests/client_roundtrip_test.rs` — `completes_the_initialize_handshake`, `returns_the_definition_location`, `returns_all_reference_locations`, `returns_hover_markdown`, `returns_document_symbols`, `surfaces_a_published_diagnostic_after_opening_a_document`, `correlates_concurrent_requests_by_id`
- [x] `tddy-core/src/toolcall/lsp.rs` — `the_first_registered_lsp_executor_wins_and_none_is_returned_before_registration` (one combined test: the process-global `OnceLock` cannot be split into empty-before / first-wins without racing)

## Validation Results (green)

Per-package (scoped, per repo guidance — full-workspace `./test` rebuilds the livekit/webrtc stack and risks disk exhaustion):

- `tddy-lsp`: **20/20 pass** (4 allowlist unit, 6 registry, 8 client incl. workspace diagnostics, 2 server-body). Server-body cancel tests finish in <1s — the real body shuts the server down gracefully, so the registry's SIGTERM→SIGKILL escalation grace is never hit.
- `tddy-lsp-executor`: **2/2 pass** (rust repo available / non-rust repo unavailable).
- `tddy-core`: `toolcall::lsp` **1/1 pass**; full lib **262/262** (no regression).
- `tddy-tool-engine`: `Lsp*` dispatch, `ReadLints`→workspace-diagnostics routing, and `ReadLints` stub-fallback all pass; existing suite green (catalog-consistency test unaffected).
- `tddy-tools`: `lsp_tools` **5/5 pass** (catalog names + gate + live `PermissionServer::new` merge/hide; other 46 lib tests untouched).
- `tddy-daemon`: `cargo build` + `clippy -D warnings` clean (register + reaper + `TDDY_LSP_TOOLS` at 3 sandboxed-session env sites).
- `tddy-sandbox-app`: `cargo build` + `clippy -D warnings` clean (register-before-spawn + `TDDY_LSP_TOOLS` in the `BTreeMap` jail env when `is_available`).
- `cargo fmt --check` clean across all touched crates.

**Implementation notes:**
- Idle tracking is inline (`HashMap<LspKey, (Arc<LspService>, Instant)>`) rather than lifting `IdleTimeoutTracker` — keeps `tddy-lsp` free of a `tddy-daemon` dependency.
- `LspServerBody::run` sets the child's `current_dir` only when `root_dir` exists (a uniform runtime guard, not a test branch); `rootUri` is always sent.
- Dispatch is a **single site** in `tddy-tool-engine::execute_tool` (the choke point all three host handlers call), so the daemon HTTP, daemon sandbox-IPC, and sandbox-app paths all serve `Lsp*` once the executor is registered — no per-handler edits.
- The executor is registered in the **daemon** and **sandbox-app** (Path-B hosts), each with a 300s idle timeout and a 60s reaper loop. Standalone `tddy-coder` sessions intentionally do NOT register one, so their gate stays off (nothing would back the tools there).

## Delta summary

### `tddy-lsp` (new crate)

Pure LSP mechanics; depends on `tddy-task`, `lsp-types`, `serde`/`serde_json`,
`async-trait`, `thiserror`, `tokio`, `bytes` only. No dependency on `tddy-build` or
`tddy-core`. Modules: `allowlist`, `error`, `protocol`, `client`, `server_body`,
`registry`. Test-only `fake_lsp` binary. The registry adds the
lookup-or-spawn-by-stable-key layer that `TaskRegistry` lacks, and re-spawns servers
whose task has become terminal.

### `tddy-core`

`toolcall/lsp.rs` — `LspExecutor` trait (`is_available` + `diagnostics`/`definition`/
`references`/`hover`/`symbols` returning `serde_json::Value`), `LspQuery`, and a
process-global `OnceLock` registry, structurally identical to `toolcall/build.rs`.

### `tddy-coder`

`lsp_executor.rs` — concrete `TddyLspExecutor` choosing the rust-only allow-list in one
place (mirrors `plugin_registry()`), resolving target → `config.type` → `Language`
→ `LspRegistry`. `register()` invoked beside `build_executor::register()`.

### `tddy-tools`

`lsp_tools.rs` — `lsp_tool_catalog()` (five `RemoteToolDef`s with language-agnostic
names) + `lsp_tool_router()` reusing `dynamic_tool_router`. `PermissionServer::new()`
merges it behind the `TDDY_LSP_TOOLS` env gate.

### `tddy-daemon`

`lsp_manager.rs` — `DaemonLspManager` wrapping an `LspRegistry` built on the daemon's
shared `TaskRegistry`, plus the idle-reaper loop. Wired into `ConnectionServiceImpl`;
`ExecuteTool` dispatches the five `Lsp*` names.

### `tddy-sandbox-app`

`spawn.rs` — propagate `TDDY_LSP_TOOLS` into the host→jail env overlay.

### `tddy-tool-engine`

`lib.rs` — `tool_read_lints` becomes the first real consumer, routing to
`lsp_executor().diagnostics(...)` when available.

### `tddy-task`

Lift `IdleTimeoutTracker` here (from `tddy-daemon::relay_idle`) so `tddy-lsp` can reuse
it; re-export from the daemon to avoid duplication.

## Non-goals

- No additional languages beyond the Rust allow-list entry.
- No rename / code-actions / formatting operations.
- No cross-session server sharing (reuse is across targets within one owner).
- No new session-tool transport — LSP tool calls reuse the existing relay.
