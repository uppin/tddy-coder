# Daemon product area changelog

**Merge hygiene:** [Changelog merge hygiene](../../dev/guides/changelog-merge-hygiene.md) — newest **`##`** first; **distinct titles** when two releases share a date; single-line bullets; do not edit older sections for unrelated work.

## 2026-07-23 — `SetProjectDefaultBranch` RPC + unified default-branch resolution

- New `ConnectionService.SetProjectDefaultBranch(project_id, main_branch_ref, daemon_instance_id)` RPC stores a project's default branch (`main_branch_ref`) in `projects.yaml`, validating the ref and project up front (`INVALID_ARGUMENT`/`NOT_FOUND`) before any write and peer-forwarding by target host like `AddProjectToHost` so the default is a property of the logical project across hosts. `ProjectEntry` now carries `main_branch_ref` so clients can read it. See [project-concept.md](project-concept.md) and [git-integration-base-ref.md](../coder/git-integration-base-ref.md).
- `effective_integration_base_ref_for_project` is unified: a stored ref wins outright; a legacy project (no stored ref) resolves its default **live** from the repository (`origin/master` → `origin/main` → `origin/HEAD`) rather than a hardcoded constant. `StartSession` uses the project's stored default when the client sends no override, so the default applies to web sessions, not only Telegram.

## 2026-07-06 — `ListAgentModels` RPC + tool-session `--model`

- New `ConnectionService.ListAgentModels(agent, daemon_instance_id)` RPC enumerates a backend's models on demand, shelling out to `tddy-tools list-models --agent <agent>` and returning `{models, default_model}`. Results are cached per (agent, daemon, OS user) with a short TTL — keyed by OS user because cursor/ACP catalogs are account-specific — and the probe runs as the user with its `current_dir` set to that user's home (the daemon cwd may be unreadable after setuid). A failed probe surfaces as an RPC error, never an empty catalog. See [tool-session-model-selection.md](../web/tool-session-model-selection.md).
- `StartSession` now threads `model` into the spawned **tool** (tddy-coder) session as `--model <m>` (previously claude-cli only), so a session runs with the operator-selected model.

## 2026-07-11 — Unprivileged daemon: Linux cgroups sandbox under `User=tddy`

- `./install --systemd` now generates the unit to run the daemon as the unprivileged **`tddy`** user by default, with **`Delegate=yes`** (a writable cgroup v2 subtree for per-session sandbox scopes) and **`AppArmorProfile=tddy-daemon`** (unprivileged user namespaces on hosts like Ubuntu 24.04 where they are AppArmor-restricted). The install creates the service user/group, chowns the log + auth-storage dirs, and ships + auto-loads (`apparmor_parser -r`) the AppArmor profile before starting the service. Overriding requires `INSTALL_OVERWRITE_SYSTEMD_UNIT=1` on an existing install; `INSTALL_DAEMON_USER=root` restores the previous multi-user setuid mode. See [systemd-install.md § Unprivileged service](systemd-install.md#unprivileged-service-linux-cgroups-sandbox).
- The Linux cgroups sandbox no longer requires root or a hardcoded `/sys/fs/cgroup`: it derives its delegated cgroup base from `/proc/self/cgroup` at runtime, optionally overridden by a new commented **`sandbox_cgroup:`** block in `daemon.yaml.production`. The userns precondition is now a functional probe (real `unshare` + uid/gid mapping), so a per-binary AppArmor grant is actually detected. It still fails fast (`FAILED_PRECONDITION`) rather than degrading to an unconfined process.

## 2026-07-16 — Shared PTY crate; Bash tool login shell

- The daemon's PTY runtime and registry moved into a shared `tddy-pty` crate (reused by tddy-coder for its bash terminal tabs); OS-user impersonation stays in the daemon over a thin adapter. See [terminal-sessions.md](terminal-sessions.md).
- The Bash tool (`StartTerminalSession`) now spawns the target user's passwd login shell instead of the daemon's `$SHELL`.

## 2026-07-12 — Fast session change: daemon-direct delete/signal + inspector `SessionEntry` bytes

- Session-scoped `ConnectionService` methods (tools, terminal control, VNC, screen-sharing) for a LiveKit-backed (tddy-coder) session are served by the coder's own LiveKit participant (`daemon-{instanceId}-{sessionId}`); the daemon no longer relays them. The daemon stays the bootstrap/directory authority (`StartSession`/`ConnectSession`/`ResumeSession` + `ListSessions`/`ListProjects`/…).
- `DeleteSession` / `SignalSession` are **daemon-direct**: the web calls them on `daemon-{instanceId}` with the caller's `session_token`; the coder is not on the path, so lifecycle control still works when the coder participant is stuck. Daemon errors surface verbatim. A contract test guards the daemon-direct path.
- `SessionEntry` gains `bytes_in` / `bytes_out` / `last_data_received_at`; the daemon populates them from the `GrpcSessionTerminal` traffic meter for claude-cli/cursor-cli/workspace sessions and reports zero/empty for stopped tddy-coder sessions, so the web inspector can render traffic for sessions with no LiveKit participant.
- Non-LiveKit (claude-cli / cursor-cli / workspace) sessions' `ConnectionService` path is unchanged.
- Feature: [terminal-sessions.md § Session-scoped RPC routing & daemon-direct lifecycle](terminal-sessions.md#session-scoped-rpc-routing--daemon-direct-lifecycle). PR [#297](https://github.com/uppin/tddy-coder/pull/297).
## 2026-07-06 — Cursor CLI sandbox parity with Claude CLI

- **`session_type = "cursor-cli"` + `sandbox = true`** succeeds on macOS (Seatbelt) and Linux (cgroups+namespaces) via `start_sandboxed_cursor_cli_session` and `tddy-sandbox-recipes::cursor_cli`; managed codebase, specialized subagents, and `TDDY_SOCKET` workflow wiring mirror claude-cli.
- In-jail `agent` spawns via direct `node index.js`; MCP config via `$HOME/.cursor/mcp.json` (no auto-injected `--approve-mcps` / `--force` / `--trust`).
- **`WaitingForInput`** remains unmapped (documented gap); sandboxed cursor-cli resume relaunch and jail Keychain auth are follow-ups. Feature: [cursor-cli-session.md](cursor-cli-session.md). PR [#287](https://github.com/uppin/tddy-coder/pull/287).

## 2026-07-05 — Cursor Agent CLI session

- **`session_type = "cursor-cli"`** — web **Create session** pane, RPC start/resume/connect, gRPC terminal I/O (same path as claude-cli), per-worktree **`.cursor/hooks.json`** → `ReportSessionStatus`, curated model catalog via **`ListAgentModels("cursor-cli")`**, Telegram **`/start-cursor`**. Sandbox and **`WaitingForInput`** are out of scope for v1. Feature: [cursor-cli-session.md](cursor-cli-session.md).

## 2026-07-04 — Durable web session (refresh token + RPC token gate)

- Session tokens split into a short-lived **access** token (5 min, unchanged, sent on every RPC) and a new long-lived **refresh** token (7-day sliding); `ExchangeCode` now mints both, and `RefreshSession` consumes a refresh token to mint a new access token plus a slid refresh token — fixing users being logged out whenever a device slept or a tab was backgrounded past the 5-minute access-token TTL
- Kind is enforced both ways: the daemon's per-RPC resolver rejects a refresh-kind token, and `RefreshSession` rejects an access-kind token, so neither credential can do the other's job; a token with no `kind` (pre-upgrade) still verifies as access-kind
- Web client gates RPC calls behind a request-time-fresh access token (`sessionTokenStore` + `authGateInterceptor`, single-flight refresh) instead of relying solely on a background timer, so the first call after waking transparently refreshes rather than failing; a top-bar indicator shows while a refresh is in flight
- Feature: [session-auth.md § Durable sessions](session-auth.md#durable-sessions-access--refresh-tokens)

## 2026-07-04 — Cross-daemon session authentication

- App session tokens are now stateless HMAC-SHA256-signed tokens (`v1.<payload>.<tag>` carrying the GitHub identity + `iat`/`exp`) signed with the shared `livekit.api_secret`, so a token minted by one daemon is verifiable by every daemon in the room — fixing `invalid or expired session` when switching daemons in the web UI and the silently-broken peer `ListProjects`/`StartSession`/`AddProjectToHost` forwarding paths
- New `RefreshSession` RPC re-mints a fresh token from a valid one; the web client refreshes every 4 min ahead of the 5-min TTL; logout is client-side; with no `livekit.api_secret` configured, auth is fail-closed (minting errors, every token rejected)
- Removed the previous per-daemon opaque-UUID / in-memory-map / `auth-sessions.json` session model (server-side logout and disk persistence are no longer meaningful for stateless tokens)
- Feature: [session-auth.md](session-auth.md)

## 2026-06-29 — Unified actions → tasks with optional sandbox execution

- New `tddy-actions` crate unifies subprocess, PTY, and pipeline execution behind `ActionSpec`; all long-running daemon work registers in the shared `TaskRegistry` (`ProcessRuntime`, `PtyRuntime`, `PipelineRuntime`)
- `actions.ActionService` RPC (`ListActionKinds`, `StartAction`, `GetAction`) complements existing `tasks.TaskService`; PTY terminals and action tasks appear in `ListTasks`
- Optional `ActionSpec.sandbox` runs confined process or runner-PTY actions via `sandbox_plan_builder` + `tddy-sandbox-recipes`; `SandboxSpec.cwd` and `extra_read_paths` wire working directory and read-only mounts; unsupported hosts return `failed_precondition`
- Session-action async jobs, `tddy-build` executor, fast tools, and sandboxed `tddy-coder` all share the same task model (`job_id == task_id`)
- Feature: [background-tasks.md](background-tasks.md), [terminal-sessions.md](terminal-sessions.md), [claude-cli-session.md](claude-cli-session.md). PR [#244](https://github.com/uppin/tddy-coder/pull/244)

## 2026-06-28 — Linux cgroups sandbox + cross-platform sandboxed sessions

- Sandboxed `claude-cli` sessions now run on **Linux** via a rootless jail (`tddy-sandbox-cgroups`): unprivileged user namespace + network namespace (loopback-only egress, forcing the in-jail `HTTPS_PROXY`) + private mount namespace + cgroup v2 limits
- `spawn_sandbox_runner` dispatches darwin (Seatbelt) / linux (cgroups) by target OS; on Linux the in-jail gRPC `SessionChannel` is served over **AF_UNIX** (survives the network namespace), dialed via `connect_sandbox_client_uds`
- Fails fast with `failed_precondition` when the host lacks unprivileged user namespaces or a writable cgroup v2 subtree — no silent unconfined fallback (production daemon runs as root systemd, where the restriction doesn't apply)
- In-jail runner + host-side relay extracted to a shared `tddy-sandbox-runner` crate; the CONNECT egress shim now waits for the host to attach before relaying (fixes an early-tunnel race)
- Sandbox opt-in exposed in the tddy-web new-session form (the `tddy-tools pty-relay --sandbox` CLI flag already existed)
- Feature: [claude-cli-session.md](claude-cli-session.md). Technical: [tddy-sandbox architecture](../../../packages/tddy-sandbox/docs/architecture.md). Known follow-ups: `pivot_root` filesystem write-confinement, config-driven cgroup limits

## 2026-06-27 — Darwin-sandboxed Claude CLI sessions (local gRPC)

- `StartSessionRequest.sandbox`: when `session_type:"claude-cli"` and `sandbox:true` on macOS, spawns `claude` inside Seatbelt via `tddy-tools sandbox-runner`; host daemon dials in-jail `SessionChannel` for PTY I/O, MCP tool exec, and LLM egress relay
- New crates `tddy-sandbox` (trait + context dir) and `tddy-sandbox-darwin` (SBPL profile + `sandbox-exec` spawn); non-macOS returns `failed_precondition`
- `ResumeSession` / `DeleteSession` stop the sandbox child and tear down the worktree; `.session.yaml` records `sandbox: true`
- Feature: [claude-cli-session.md](claude-cli-session.md). Technical: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md#sandboxed-claude-code-cli-sessions), [tddy-sandbox architecture](../../../packages/tddy-sandbox/docs/architecture.md).

## 2026-06-26 — Browser DEBUG mask — config-driven terminal diagnostics

- `DaemonConfig.debug: Option<String>` threaded through `run_server` → `ClientConfig.debug` and served at `GET /api/config`; browser picks up the mask for scoped `[tddy]` console logging
- `dev.daemon.yaml` ships `debug: "tddy:term:*"` — covers all terminal namespaces; comment out or set `""` to disable

## 2026-06-26 — PTY terminal width fix — correct cols/rows on gRPC reconnect

- `StreamTerminalOutputRequest` now accepts `initial_cols`/`initial_rows`; when non-zero the daemon resizes the PTY, drains stale broadcast output, and triggers a SIGWINCH redraw before forwarding live output — eliminates 220-column garbling on browser reconnect
- `PtyHandle::send_input` strips `\x1b]resize;{cols};{rows}\x07` OSC escape sequences from stdin data and calls `PtyHandle::resize` transparently
- `kill_all()` on daemon shutdown: sends SIGTERM to every registered PTY process, waits up to 5 s, then SIGKILL for survivors; clears the registry
- Capture replay buffer limit raised 64 KB → 512 KB (more history for reconnecting clients)
- New `tddy-demo-tui` binary: reads PTY dimensions via TIOCGWINSZ, draws `DEMO TUI W={cols} H={rows}`, redraws on SIGWINCH — used as fake claude CLI in e2e tests
## 2026-06-26 — Single-screen terminal control mutex

- Per-session exclusive control lease in `ClaudeCliSessionManager`: first browser tab to attach becomes the controller; subsequent tabs see a **"Claim terminal"** overlay and cannot send input
- New `ConnectionService` RPCs: `ClaimTerminalControl` (unary, `steal` flag to evict the current holder) and `WatchTerminalControl` (server-stream, snapshot-then-delta via broadcast channel)
- `control_token` field added to `SessionTerminalInput`, `SignalSessionRequest`, `StartTerminalSessionRequest`, `StopTerminalSessionRequest`; input RPCs return `FAILED_PRECONDITION` when the token is wrong
- Uncontrolled sessions (no lease held) accept all inputs — fully backwards compatible

## 2026-06-25 — Multiple tools per session (Bash tool)

- A session can run multiple identified tools, not just `claude`: the main terminal is the reserved id `"main"` (kind `"claude-cli"`); on-demand **Bash** tools (kind `"bash"`) run `$SHELL` (fallback `/bin/bash`) in the worktree, no inputs
- New `ConnectionService` RPCs `StartTerminalSession` / `StopTerminalSession` / `ListTerminalSessions` (`TerminalSessionInfo{terminal_id, kind, pid}`); stopping `"main"` is rejected with `INVALID_ARGUMENT`
- Terminal I/O RPCs (`StreamSessionTerminalIO`, `StreamTerminalOutput`, `SendTerminalInput`) gain an optional `terminal_id` (empty ⇒ `"main"`); unknown id → `NOT_FOUND`
- RPC-only; no web UI integration in this release

## 2026-06-24 — Long-running background Tasks

- New `tasks.TaskService` gRPC service: `ListTasks`, `GetTask`, `WatchTask` (replay-then-live stream with `is_replay` flag), `CancelTask`, `SendInput`
- Every `ExecuteTool` invocation (fast Read/Write/etc.) registers a `Task` in the shared `TaskRegistry`; background Shell tasks observable via `WatchTask`; `Await` tool blocks on `TaskRegistry`
- VM image builds (Buildroot) are now cancellable tasks: `CancelTask` sends SIGINT to the `make` PID; build failure maps to `TaskStatus::Failed` (not `Completed`)
- Cooperative cancellation via `tokio_util::CancellationToken` per task; SIGTERM→SIGKILL escalation safety net after 5 s grace period
- Terminal tasks retained for 5 minutes then evicted; registry capped at 200 terminal tasks (oldest-first eviction)
- Minimal `/tasks` web page: 3-second polling, colour-coded status, Cancel button
- Feature: [daemon/background-tasks.md](background-tasks.md). Cross-package: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-06-21 — Demo goal Phase 2: daemon VM lifecycle RPCs

- `StartDemoVm` RPC: reads session's `demo-plan.md`, builds `DemoVmConfig`, spawns `QemuDemoVm::boot()` background task, tracks handle per session
- `StopDemoVm` RPC: removes handle and calls `shutdown()` via monitor socket
- `GetDemoVmStatus` RPC: returns `DemoVmState` (`BOOTING`/`RUNNING`/`STOPPED`/`ERROR`), `ssh_host_port`, and `share_url`
- Feature: [coder/demo-goal.md](../coder/demo-goal.md). Cross-package: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-06-15 — RPC Playground

- **Backend**: `grpc.reflection.v1.ServerReflection` service (`reflection_service.rs`, vendored `reflection.proto`, embedded `FileDescriptorSet` from build.rs); `MultiRpcService::service_names()` in `tddy-rpc`; `reflection_entry_from()` helper registered in daemon `main.rs` and all `tddy-coder` `MultiRpcService` sites; daemon spawns a dedicated `LiveKitParticipant` in the common room (identity `daemon-{id}`) so the playground reaches it via data channel.
- **Frontend** (`tddy-web`): `/rpc-playground` route (hash-based, no 404 on reload); participant picker filtered to `coder`-role only; `RpcPlaygroundScreen` + `RpcPlaygroundAppPage`; request editor (builder ↔ raw JSON, synced); streaming panel; `invoke.ts` auto-injects `sessionToken` when the request type has a `session_token` field.
- **Reflection codegen** (`tddy-livekit-web`): `reflection_pb.ts` generated via buf; `createLiveKitTransport` used for all reflection + invocation calls (avoids fetch streaming body limits).
- **Test infrastructure**: Cypress tests in `tddy-livekit-web` and `tddy-web` auto-start a Docker LiveKit container when `LIVEKIT_TESTKIT_WS_URL` is not set.
- Feature: [rpc-playground.md](rpc-playground.md). Cross-package: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-06-14 — Remote-codebase mode

- **Remote daemon**: workspace sessions (`session_type:"workspace"`) with git worktree, no PTY; `ExecuteTool` (Read, Write, StrReplace, Delete, Grep, Glob, Shell, Await, SemanticSearch, ReadLints) + `ListExecTools` RPCs; `contain_path` security; background shell jobs + Await polling.
- **Relay daemon** (`--relay`): joins LiveKit common room; `forward_to_peer` + per-peer `RpcClient` cache routes `ExecuteTool`/`ListExecTools` to named remote peer; `IdleTimeoutTracker` triggers graceful shutdown after idle timeout; external oneshot shutdown channel in `run_server`.
- **`tddy-tools remote`**: `list-tools` via `ListExecTools` Connect POST; `start-session`, `connect-session`, `sync-context` subcommands; lazy relay daemon startup via `ensure_relay_daemon`.
- **`tddy-coder --remote`**: `--remote-daemon-url`/`--remote-session-token`/`--remote-daemon-id` flags; `run_remote` shells out to `tddy-tools remote list-tools`, builds dynamic `mcp__tddy-tools__*` allowlist, runs free-prompting workflow with remote ctx keys and read-only local ctx dir.
- Feature: [remote-codebase-mode.md](remote-codebase-mode.md). Cross-package: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-06-13 — Claude Code CLI permission mode selection

- **`tddy-service`**: `StartSessionRequest.permission_mode` (proto field 14, string).
- **`tddy-daemon`**: `build_claude_argv` appends `--permission-mode <mode>` (5th param; `None`/empty/whitespace → `auto`); `ClaudeCliSessionManager::start()` accepts `permission_mode: Option<&str>` (6th param); `connection_service::start_session` extracts and trims `req.permission_mode`, passes through `start_claude_cli_session` → `manager.start` → `build_claude_argv`. Tests: `claude_cli_permission_mode_acceptance` (16 tests). **`tddy-tools`**: `pty-relay --permission-mode` optional CLI arg wired into `StartSessionRequest`. Feature: [claude-cli-permission-mode.md](claude-cli-permission-mode.md). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).
## 2026-06-13 — Per-worktree hooks: claude-cli session activity status

- **`tddy-core`**: **`session_activity`** — **`SessionActivityStatus`** enum (`Started`, `Running`, `ExecutingTool`, `WaitingForInput`, `Done`, `Ended`) with `as_wire()`/`from_wire()`; **`activity_status_from_hook(event, notif_type)`** maps Claude Code hook events; **`HookEvent`** serde struct + `parse_hook_event`; 15 unit tests. **`claude_hooks`** — **`HookCommandParams`**, **`build_claude_hooks_settings()`** builds the 6-event settings JSON; 4 unit tests. **`session_metadata`** — **`activity_status: Option<String>`** and **`hook_token: Option<String>`** on **`SessionMetadata`** (serde-default, backward-compat); **`update_activity_status()`** read-modify-write helper.
- **`tddy-service`**: **`connection.proto`** — **`ReportSessionStatus`** RPC; **`ReportSessionStatusRequest/Response`** messages; **`SessionEntry.activity_status`** (field 15); **`StartSessionRequest.initial_prompt`** (field 13).
- **`tddy-daemon`**: **`connection_service`** — **`report_session_status`** handler (path-traversal guard, `os_user` sessions_base, constant-time `hook_token` check, `update_activity_status`); hook wiring in **`start_claude_cli_session`** (UUID token, resolves `tddy_tools_path`/`daemon_url`, writes `<worktree>/.claude/settings.local.json`, persists `hook_token` in metadata); **`session_list_enrichment`** surfaces `activity_status` via **`ListSessions`**. **`config`** — `tddy_tools_path`, `daemon_url` on **`ClaudeCliConfig`**. 6 handler unit tests. **`ClaudeCliSessionManager`** extracted as constructor parameter; `build_claude_argv()` helper; **`PtyHandle::resize()`** + `current_size`.
- **`tddy-tools`**: **`session-hook`** subcommand — reads stdin JSON, maps event, POSTs **`ReportSessionStatus`**; fail-quiet (always exit 0, 2s timeout); 5 CLI acceptance tests. **`pty_relay`** — `encode_resize()` corrected to OSC format `\x1b]resize;{cols};{rows}\x07`.
- **Feature docs**: [claude-cli-session.md](claude-cli-session.md#session-activity-status-via-per-worktree-hooks); technical: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md#claude-code-cli-sessions). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-06-06 — Session chaining: stable parent id in Telegram callback

- **`tddy-daemon`**: **`telegram_session_control`** — **`tcp:`** chain callback format changed from `tcp:<idx>|s:<child>` to `tcp:p:<parent_tail8>|s:<child>` (last 8 chars of parent session id); **`handle_chain_parent_callback`** scans candidates by tail instead of index position — stable across session churn between keyboard render and tap; **`session_tail8()`** helper; **`parse_telegram_chain_parent_callback`** signature updated to return `(String, String)`. Unit tests: **`parse_chain_workflow_prompt`** (strip/trim/wrong-prefix), **`parse_telegram_chain_parent_callback`** round-trip, empty-tail rejection, **`session_tail8`** boundary cases. **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-06-06 — Claude Code CLI session type

- **`tddy-service`**: **`connection.proto`**: `session_type` (field 7) and `model` (field 8) on **`StartSessionRequest`**; **`StreamSessionTerminalIO`** bidi RPC, **`SessionTerminalInput`** and **`SessionTerminalOutput`** messages.
- **`tddy-core`**: **`SessionMetadata`** gains **`session_type: Option<String>`** and **`model: Option<String>`**; **`InitialToolSessionMetadataOpts`** extended; **`write_initial_claude_cli_session_metadata()`** convenience wrapper; backward-compatible YAML serde (missing fields → `None`). Tests: **`claude_cli_metadata_round_trip`**.
- **`tddy-daemon`**: **`claude_cli_session`** — **`ClaudeCliSessionManager`** (tokio-channel subprocess registry; `start()` spawns `claude --model <m> --session-id <session_id>`; `resume()` relaunches in same worktree; background exit monitor; broadcast stdout, mpsc stdin); **`connection_service`** — `start_session` claude-cli branch (worktree creation, metadata write, `ClaudeCliSessionManager::start`, empty LiveKit response), `connect_session` early return for claude-cli, `stream_session_terminal_io` bidi handler, `delete_session` worktree cleanup; **`session_list_enrichment`** populates `agent = "claude-cli"` and `model` from metadata; **`config`** optional `claude_cli.binary_path`. Tests: `claude_cli_session_acceptance` (`claude_cli_session_metadata_fields_persisted`, `claude_cli_session_livekit_fields_empty`, `claude_cli_session_enrichment_reads_from_metadata`, `claude_cli_session_resume_relaunches_in_worktree`, `claude_cli_start_session_requires_model`).
- **`tddy-web`**: **`ConnectionScreen`** session type selector + model dropdown; **`ConnectedClaudeCliTerminal`** bidi gRPC stream component; **`GhosttyTerminalGrpc`** (`GrpcStream` interface, output buffer before ready, OSC resize, optional chrome bar); **`constants/claudeCliModels.ts`** (`CLAUDE_CLI_MODELS`, `isClaudeCliSession`); `multiSessionState` extended with optional `claudeCli` discriminant. Tests: `claudeCliModels.test.ts`, `GhosttyTerminalGrpc.cy.tsx`.
- **Feature docs**: [claude-cli-session.md](claude-cli-session.md); technical: [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md), [web-terminal.md](../web/web-terminal.md). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-05-02 — Session chaining (`/chain-workflow`, `tcp:` callbacks, chain base merge)

- **`tddy-daemon`**: **`handle_chain_workflow`** creates the child session, lists parent rows via **`parent_candidates_page_for_chain_picker`**, sends **`tcp:<idx>|s:<child_session_id>`** buttons, then the recipe keyboard; **`parse_telegram_chain_parent_callback`**, **`handle_chain_parent_callback`** (child id validated with **`validate_session_id_segment`**), **`CB_TELEGRAM_CHAIN_PARENT`**. **`telegram_bot`** routes **`tcp:`** through **`maybe_dispatch_tcp_chain_parent_callback`** (answers the callback query and sends **`telegram_workflow_error_message`** text on failure, same pattern as **`intent:`** / **`tp:`** workflow steps); **`workflow_callback_gate_authorized`** centralizes allowlist checks on workflow callbacks. **`spawn_telegram_workflow`** runs **`merge_chain_integration_base_with_explicit_operator_overrides`** inside **`tokio::task::spawn_blocking`** when **`.session.yaml`** carries **`previous_session_id`**. Integration **`telegram_chain_workflow_shows_parent_pick_first`**, **`telegram_chain_parent_tap_persists_previous_session_id_on_child`**, **`parent_candidates_page_for_chain_picker_excludes_child_and_caps_page`**, **`telegram_chain_parent_callback_rejects_invalid_child_session_id_segment`**; **`telegram_bot_rs_dispatches_chain_workflow_command`**; **`session_chaining_phase2_acceptance`** / **`session_chaining_phase2_unit`** guard wiring and merge behavior.
- **`tddy-core`**: **`session_chain`** (**`resolve_chain_integration_base_ref_from_parent_session`**, **`integrate_chain_base_into_session_worktree_bootstrap`**); parent **`repo_path`** required when parent **`changeset.yaml`** names a branch; **`SessionMetadata.previous_session_id`** / **`InitialToolSessionMetadataOpts`**; **`slash_menu_entries`** includes **`/chain`**. Tests: **`session_chain_acceptance`**, **`session_metadata::chain_child_metadata_records_previous_session_id`**, **`session_chain`** unit tests.
- **`tddy-tui`**: **`ViewState::chain_workflow_parent_picker_active`** clears when **`AppMode`** is not **`FeatureInput`** (**`on_mode_changed`**). Tests: **`chain_phase2_acceptance`**, **`chain_phase2_unit`**.
- **Feature docs**: [telegram-session-control.md](telegram-session-control.md), [git-integration-base-ref.md](../coder/git-integration-base-ref.md), [session-layout.md](../coder/session-layout.md), [feature-prompt-agent-skills.md](../coder/feature-prompt-agent-skills.md). Optional follow-ups: [2026-05-02-changeset-session-chaining.md](../../dev/1-WIP/2026-05-02-changeset-session-chaining.md). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-05-02 — Telegram tracked session gate and chat traffic logs

- **`tddy-daemon`**: **`telegram_tracked_session`** — per-chat optional **`session_id`** binding (**`SharedTelegramTrackedSessionCoordinator`**) shared with **`TelegramSessionWatcher`** and **`telegram_session_control`**; presenter **`ModeChanged`** workflow keyboards suppress under **no / mismatched** tracking with **Enter session** fallback; **queue promotion replay** bypasses the gate; **Enter** binds + **elicitation replay**; clears on **WorkflowComplete**, matching **delete**, or explicit per-chat clear. Structured **`telegram_traffic`** logs on **`tddy_daemon::telegram`**; inbound message/callback summaries on **`tddy_daemon::telegram_bot`**. Integration **`telegram_tracked_session_acceptance`**; concurrent + multi-select suites bind tracking where full keyboards are asserted.
- **Feature docs**: [telegram-session-control.md](telegram-session-control.md), [telegram-notifications.md](telegram-notifications.md). **Package**: [telegram-notifier.md](../../packages/tddy-daemon/docs/telegram-notifier.md), [changesets.md](../../packages/tddy-daemon/docs/changesets.md). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-05-02 — Telegram MultiSelect shortcuts (`eli:mn:` / `eli:mr:`)

- **`tddy-daemon`**: **`telegram_multi_select_shortcuts`** — compact **Choose none** (**`eli:mn:`**) and **Choose recommended** (**`eli:mr:`**, when **`recommended_other`** is present) keyboards within Telegram’s **64-byte** **`callback_data`** limit; **`TelegramSessionWatcher`** **`MultiSelectShortcutElicitationMeta`** cache keyed by Telegram chat plus session (**`recommended_other`** for **Choose recommended**); **`telegram_bot`** dispatches **`eli:mn:`** / **`eli:mr:`** through **`authorized_elicitation_surface_gate`**; **`handle_elicitation_multi_select_shortcut`** submits **`PresenterIntent::AnswerClarificationMultiSelect`**. Integration tests **`telegram_multi_select_acceptance`**; **`telegram_concurrent_elicitation_integration`** asserts primary-keyboard alignment for MultiSelect shortcuts.
- **`tddy-core`**: Presenter rejects **`AnswerClarificationMultiSelect`** with empty indices and no **Other** text when **`allow_other`** on the clarification is **false**.
- **`tddy-service`**: **`ClarificationQuestionProto.recommended_other`** on MultiSelect wire events.
- **Feature docs**: [telegram-session-control.md](telegram-session-control.md), [telegram-notifications.md](telegram-notifications.md). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-04-11 — Operator OAuth loopback tunnel (daemon)

- **`tddy-daemon`**: **`oauth_loopback_tunnel`** — **`TcpListener`** on operator **`127.0.0.1:{callback_port}`**, **`RpcClient::start_bidi_stream`** **`loopback_tunnel.LoopbackTunnelService`/`StreamBytes`**, **`pick_daemon_oauth_target`** over common-room **`daemon-*`** metadata; **`run_oauth_tunnel_supervisor_follow_room_slot`** with **`livekit_peer_discovery`**; **`codex_oauth_participant_metadata`**. Package **[oauth-loopback-tunnel.md](../../packages/tddy-daemon/docs/oauth-loopback-tunnel.md)**; feature **[codex-oauth-relay.md](codex-oauth-relay.md)**, **[livekit-peer-discovery.md](livekit-peer-discovery.md)**. **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-04-11 — LiveKit common-room peer discovery and cross-daemon StartSession

- **`tddy-daemon`**: Module **`livekit_peer_discovery`** — JSON metadata advertisement, **`CommonRoomPeerRegistry`**, **`LiveKitEligibleDaemonSource`**, **`LiveKitDiscoveryHandles`**, background join/sync for **`livekit.common_room`**, **StartSession** forward via **`tddy_livekit::RpcClient`** to peer identity; **`local_instance_id_for_config`** shared with **ConnectionService**; **`TDDY_PROJECTS_DIR`** test hook documented on **`projects_path_for_user`**. Integration tests **`livekit_peer_daemons_acceptance`**, **`multi_host_acceptance`** (remote routing). **`tddy-livekit`**: **`RpcClient::new_shared`** (**`Arc<Room>`**).
- **Feature doc**: [livekit-peer-discovery.md](livekit-peer-discovery.md) (includes operator / CI notes). **Web**: [web-terminal.md](../web/web-terminal.md) (eligible daemons, host ordering). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).
## 2026-04-11 — Connection service: project entries with owning daemon and peer row hook

- **`connection.proto`**: **`ProjectEntry.daemon_instance_id`** identifies the registry row’s owning daemon.
- **`tddy-daemon`**: **`list_projects`** merges local disk projects with **`EligibleDaemonSource::peer_project_entries(session_token)`**; the default **`EligibleDaemonSource`** supplies an empty peer list. Integration test **`list_projects_multi_daemon_aggregation`** exercises merge and per-row **`daemon_instance_id`**. Cross-package note: **[docs/dev/changesets.md](../../dev/changesets.md)**; web feature doc: **[web-terminal.md](../web/web-terminal.md)** (eligible daemons / **`ListProjects`**).

## 2026-04-06 — Telegram user ↔ GitHub identity (library)

- **`tddy-daemon`**: Module **`telegram_github_link`** — **`TelegramOAuthStateSigner`** (HMAC-SHA256 OAuth **`state`** bound to **`telegram_user_id`**), **`TelegramGithubMappingStore`** (JSON on disk, atomic replace), **`resolved_os_user_for_telegram_workflow`**, **`complete_telegram_link_via_stub_exchange`** (**`StubGitHubProvider`**). **`TelegramSessionControlHarness::with_telegram_github_link`** optional mapping path; **`handle_start_workflow`** rejects unlinked Telegram users when that path is set (error text references **`/link-github`** / web OAuth). Dependencies: **`base64`**, **`hmac`**, **`sha2`**, **`subtle`**.
- **Feature doc**: [telegram-session-control.md](telegram-session-control.md). Package: [telegram-github-link.md](../../packages/tddy-daemon/docs/telegram-github-link.md), [changesets.md](../../packages/tddy-daemon/docs/changesets.md).

## 2026-04-06 — Telegram: concurrent elicitation (one chat, active token)

- **Coordinator:** **`ActiveElicitationCoordinator`** maintains a per-chat FIFO queue of workflow sessions; the head session owns the **active elicitation token** for Telegram interactive surfaces.
- **Outbound:** **`TelegramSessionWatcher`** registers elicitation requests on **`ModeChanged`**; sessions that are not primary for a chat receive a **deferred** text notice without a competing full **`eli:s:`** inline keyboard.
- **Inbound:** **`telegram_bot`** applies the same **active-token** policy to **`eli:s:`**, **`eli:o:`**, **`eli:mn:`**, **`eli:mr:`**, and **`doc:`** callbacks; **`/answer-text`** and **`/answer-multi`** check the active session before **`PresenterIntent`** calls. **`telegram_session_control`** advances the queue after completion on select, Other follow-up, multi-select shortcuts, applicable document-review actions, and successful text/multi answers.
- **Observability:** Deep per-chat queues trigger a **warning** log at a fixed depth threshold.
- **Feature docs:** [telegram-session-control.md](telegram-session-control.md), [telegram-notifications.md](telegram-notifications.md). Package: [telegram-notifier.md](../../packages/tddy-daemon/docs/telegram-notifier.md), [changesets.md](../../packages/tddy-daemon/docs/changesets.md).

## 2026-04-06 — Telegram `/start-workflow`: branch/worktree intent step

- **`tddy-daemon`**: After a recipe is saved (excluding **More recipes** follow-up), the bot prompts for **branch/worktree intent** (**New branch + worktree** vs **Work on existing branch**). The choice is written to **`changeset.yaml`** under **`workflow.branch_worktree_intent`** (`new_branch_from_base` / `work_on_selected_branch`) before project selection. Inline **`callback_data`** uses compact **`intent:nb|s:<session_id>`** and **`intent:ws|s:<session_id>`** so payloads stay within Telegram’s 64-byte limit with a UUID session id.
- **Feature doc**: [telegram-session-control.md](telegram-session-control.md). Package history: [changesets.md](../../packages/tddy-daemon/docs/changesets.md).

## 2026-04-05 — Telegram: inbound session control, PresenterIntent, elicitation UX

- **Inbound control**: Daemon runs **`telegram_bot`** (teloxide long-polling) when Telegram is configured and **`sessions_base`** resolves. Commands include **`/start-workflow`**, **`/sessions`**, **`/delete`**, **`/submit-feature`**, **`/answer-text`**, **`/answer-multi`**; callbacks cover session list, recipe/project/agent picks, document review (**`doc:`**), and elicitation select (**`eli:s:`**). **`TelegramSessionControlHarness`** and integration tests exercise the library; production uses **`TeloxideSender`** with the same bot as outbound notifications.
- **PresenterIntent**: **`presenter_intent.proto`** and **`tddy-daemon::presenter_intent_client`** forward answers and document actions to the child **`tddy-coder`** on localhost gRPC.
- **Outbound notifications**: **`ModeChanged`** for document review / markdown viewer sends **full document body** (chunked), then **Approve** / **Reject** / **Refine** (and related) inline actions. **`Select`** clarification sends a **numbered option list** in the message body, **numeric** inline buttons, and a **post-tap confirmation** with the full chosen option text. Dedupe for identical **`ModeChanged`** payloads per session is unchanged.
- **Formatting**: Styled text must follow Telegram **[message entities](https://core.telegram.org/api/entities)** rules (UTF-16 code units for offsets and lengths where applicable).
- **Feature docs**: [telegram-session-control.md](telegram-session-control.md), [telegram-notifications.md](telegram-notifications.md).

## 2026-04-05 — Telegram extended recipe keyboard: `review`

- **`tddy-daemon`**: **`RECIPE_MORE_PAGE`** includes the **`review`** workflow recipe name (same normalization rules as other CLI recipe strings).
- **Cross-reference**: [workflow-recipes.md](../coder/workflow-recipes.md) (**Selecting a recipe**); package [changesets.md](../../packages/tddy-daemon/docs/changesets.md).

## 2026-04-04 — Session elicitation: Telegram `ModeChanged` + `ListSessions` flag

- **`connection.proto`**: **`SessionEntry.pending_elicitation`** (field **14**).
- **`tddy_core`**: **`SessionMetadata.pending_elicitation`** in **`.session.yaml`** (serde default **`false`**).
- **`tddy-daemon`**: Module **`elicitation`** — list flag from metadata; **`TelegramSessionWatcher::on_server_message`** handles **`ModeChanged`** with dedupe and generic approval/input Telegram lines; **`session_list_enrichment`** sets the proto field. Tests: **`telegram_notifier`** acceptance unit tests, **`list_sessions_enriched`**, **`session_list_enrichment`** unit test.
- **Feature docs**: [telegram-notifications.md](telegram-notifications.md) (Presenter stream: elicitation); [web-terminal.md](../web/web-terminal.md) (pending elicitation on rows). Package: [telegram-notifier.md](../../packages/tddy-daemon/docs/telegram-notifier.md), [changesets.md](../../packages/tddy-daemon/docs/changesets.md). Cross-package: **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-05 — Documentation wrap (telegram presenter PRD retired)

- **Docs**: WIP PRD for Telegram **PresenterObserver** stream removed from **`docs/ft/daemon/1-WIP/`**; product and integration remain in [telegram-notifications.md](telegram-notifications.md). **`docs/dev/1-WIP/daemon-telegram-validate/`** report bundle removed. Cross-package note: **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-04 — Projects: `main_branch_ref` (git integration base)

- **Registry**: Optional **`main_branch_ref`** on project rows; **`effective_integration_base_ref_for_project`**; **`add_project`** rejects invalid refs before **`projects.yaml`** writes (**`tddy_core::validate_integration_base_ref`**).
- **Docs**: [git-integration-base-ref.md](../coder/git-integration-base-ref.md), [project-concept.md](project-concept.md); package [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md).
- **PRD retired**: Prior WIP PRD for the multi-user daemon was merged into [project-concept.md](project-concept.md) (**Multi-user daemon**) and this changelog; source file removed from **`docs/ft/daemon/1-WIP/`**.

## 2026-04-04 — Worktrees library + ConnectionService RPCs

- **`tddy_daemon::worktrees`**: Parses **`git worktree list`** output; **`WorktreeStatsCache`** persists per-project snapshots under **`TDDY_PROJECTS_STATS_ROOT`** (default **`~/.tddy/projects`**); **`validate_worktree_path_within_repo_root`** (lexical containment); **`remove_worktree_under_repo`** (membership in **`git worktree list`**, refuses primary worktree).
- **ConnectionService**: **`ListWorktreesForProject`** (optional **`refresh`** → **`refresh_stats_for_project`** in **`spawn_blocking`**), **`RemoveWorktree`** (invalidates cache on success). Project path via **`main_repo_path_for_host`** and local **`daemon_instance_id`** (remote daemon routing for these RPCs is out of scope). Tests: **`worktrees`**, **`worktrees_acceptance`**, **`worktrees_rpc`** (requires **`git`**, **`USER`** for registry tests).
- **Package doc**: [worktrees.md](../../packages/tddy-daemon/docs/worktrees.md), [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md). Web feature: [worktrees.md](../web/worktrees.md).

## 2026-04-03 — Telegram session notifications (library)

- **Config**: Optional **`telegram`** block in **`daemon.yaml`** with **`enabled`**, **`bot_token`**, and **`chat_ids`** (integer chat targets); unknown keys on the block are rejected under **`deny_unknown_fields`**.
- **Behavior**: The **`tddy_daemon::telegram_notifier`** module provides **`TelegramSessionWatcher`** (baseline + one notification per status transition for active sessions), **`session_telegram_label`** (first two hyphen segments of **`session_id`**), **`mask_bot_token_for_logs`**, and **`send_telegram_via_teloxide`** (teloxide **`Bot::send_message`**). Tests use a mock **`TelegramSender`**; CI avoids the live Telegram API.
- **Docs**: Product reference **[telegram-notifications.md](telegram-notifications.md)**; technical reference **[telegram-notifier.md](../../packages/tddy-daemon/docs/telegram-notifier.md)**.

## 2026-04-03 — ConnectionService: workflow files, session base path, delete

- **`ListSessionWorkflowFiles`** / **`ReadSessionWorkflowFile`**: Allowlisted basenames (`changeset.yaml`, `.session.yaml`, `PRD.md`, `TODO.md`) under **`{sessions_base}/sessions/{session_id}/`** with canonical-path checks (**`session_workflow_files`**; tests **`session_workflow_files_rpc`**).
- **Sessions base**: **`sessions_base_for_user`** resolves the Tddy **data directory** (typically **`~/.tddy`**), matching **`tddy_core::output::tddy_data_dir_path`** when **`TDDY_SESSIONS_DIR`** is unset, so listing/connect/delete target the same trees as **`tddy-coder`**.
- **`DeleteSession`**: Terminates a live **`metadata.pid`** when needed (SIGTERM/SIGKILL; Linux zombie handling), then removes the directory; directories without readable **`.session.yaml`** are removed when the resolved path is valid.
- **Package**: [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md). Web: [web-terminal.md](../web/web-terminal.md), [web changelog](../web/changelog.md).

## 2026-03-29 — ConnectionService: `ListAgents` and `allowed_agents`

- **Config**: Daemon YAML includes **`allowed_agents`**, a list of **`id`** (required) and optional **`label`** entries (same shape as tool allowlist entries; unknown keys on each entry are rejected when using **`deny_unknown_fields`**).
- **`ListAgents`**: Returns **`AgentInfo`** rows in config order; display labels use trimmed non-empty **`label`**, otherwise **`id`**.
- **`StartSession`**: When **`allowed_agents`** is non-empty, a non-empty **`agent`** must match an **`id`**; otherwise **`INVALID_ARGUMENT`**. An empty **`allowed_agents`** list does not apply this check.
- **Implementation**: Shared mapping lives in **`agent_list_mapping`**; integration tests cover config parse, RPC payloads, **`ListTools`** regression, and unknown agent rejection.
- **Package doc**: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md). **Install / config**: [systemd-install.md](systemd-install.md).

## 2026-03-28 — Unified session tree and `session_id` validation

- **Filesystem**: Session directories use `{sessions_base}/sessions/{session_id}/` consistently for listing, connect, resume, signal, delete, and headless `GetSession` / `ListSessions`.
- **Validation**: `session_id` is validated as a single safe path segment on **ConnectSession**, **ResumeSession**, **SignalSession**, **DeleteSession**, and service-side **GetSession** before paths are built (aligned with `session_deletion` rules).
- **Feature reference**: [Session directory layout](../coder/session-layout.md) ([migration from non-unified trees](../coder/session-layout.md#migration-from-non-unified-trees)).

## 2026-03-28 — StartSession and spawn: `recipe`

- **`StartSession` / `StartSessionRequest`**: Optional **`recipe`** (`tdd` or `bugfix`); empty behaves like **`tdd`**. Session **`changeset.yaml`** persists **`recipe`** for the new session.
- **Spawn**: **`SpawnRequest`** includes **`recipe`**; the daemon passes **`--recipe`** to **`tddy-coder`** when set.
- **Package**: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md). Coder feature: [workflow-recipes.md](../coder/workflow-recipes.md).

## 2026-03-28 — ConnectionService: multi-host selection + ListSessions workflow enrichment

- **`ListEligibleDaemons`**: Returns eligible daemon entries from **`EligibleDaemonSource`** (local instance; LiveKit peer discovery deferred).
- **`ListSessions`**: Each **`SessionEntry`** includes **`daemon_instance_id`** for the owning daemon, plus **`workflow_goal`**, **`workflow_state`**, **`elapsed_display`**, **`agent`**, and **`model`** from **`.session.yaml`** / **`changeset.yaml`** via **`session_list_enrichment`**. Blocking read and enrichment run on the thread pool via **`spawn_blocking_with_timeout`**. Enrichment failures are logged at **warn**; the RPC still returns base session fields from **`session_reader`**.
- **`StartSession`**: Accepts optional **`daemon_instance_id`**; local spawn when empty or matching the local instance; non-local targets return **unimplemented** until cross-daemon spawn routing exists.
- **Proto / service**: **`connection.proto`** defines **`SessionEntry`** fields; TypeScript and Rust stubs are generated from the proto.
- **Package doc**: [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md). Web UX: [web-terminal.md](../web/web-terminal.md).

## 2026-03-24 — ConnectionService: DeleteSession

- **`DeleteSession`**: Removes the on-disk session directory under the authenticated user’s **`{sessions_base}/sessions/{session_id}/`** tree. Rejects invalid session ids with **`INVALID_ARGUMENT`**. Filesystem removal errors return a generic **`INTERNAL`** message to clients; full error detail is logged on the server.
- **Current behavior** (terminate live processes, metadata-less directories, **`sessions_base`** resolution): see **2026-04-03 — ConnectionService: workflow files, session base path, delete** above.

## 2026-03-23 — Root `./install --systemd`

- **Installer**: Repo **`./install --systemd`** (optional **`--build`** runs **`./release`** first) copies **`tddy-daemon`**, **`tddy-coder`**, **`tddy-tools`** from **`target/release/`**; installs **`daemon.yaml`** from **`daemon.yaml.production`** only when missing; writes **`tddy-daemon.service`**; copies **tddy-web** **`dist`** when present; runs **`systemctl`** unless **`INSTALL_NO_SYSTEMCTL=1`**.
- **Paths**: Overridable via **`INSTALL_PREFIX`**, **`INSTALL_BIN_DIR`**, **`INSTALL_CONFIG_DIR`**, **`INSTALL_SYSTEMD_DIR`**, **`INSTALL_WEB_BUNDLE_DIR`**.
- **Docs**: Feature summary in **[systemd-install.md](systemd-install.md)**. Example unit: **[docs/dev/tddy-daemon.service.example](../../dev/tddy-daemon.service.example)**.

## 2026-03-22 — LiveKit: `livekit.common_room` for spawns

- When **`livekit.common_room`** is set (non-empty), daemon-spawned **`tddy-*`** processes receive **`--livekit-room`** set to that value so all sessions share one room; **`--livekit-identity`** remains **`daemon-{session_id}`** per session. If unset or whitespace-only, the room name is **`daemon-{session_id}`** as before.

## 2026-03-21 — StartSession: `agent`

- **ConnectionService**: `StartSessionRequest` includes optional `agent`; forwarded to spawned `tddy-coder` as `--agent` when non-empty (skips interactive backend menu in the child).

## 2026-03-21 — Project concept

- **Projects**: Named `git_url` + `main_repo_path` per user; `~/.tddy/projects/projects.yaml`.
- **Config**: `repos_base_path` (default `repos` under user home).
- **ConnectionService**: `ListProjects`, `CreateProject` (optional `user_relative_path` for clone/adopt location under `~`); `StartSession` uses `project_id`; `SessionEntry` includes `project_id`.
- **Clone**: On create, clone into `{repos_base}/{name}/` unless path exists (then adopt).
- **Spawn**: `tddy-coder` receives `--project-id`; `.session.yaml` stores `project_id`.
- **PRD reference:** PRD-2026-03-21-project-concept.md (wrapped into [project-concept.md](project-concept.md)).
