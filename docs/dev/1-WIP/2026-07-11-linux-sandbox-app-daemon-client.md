# Changeset: Linux `tddy-sandbox-app` → daemon-assisted sandbox over tonic UDS

**PRD**: `docs/ft/coder/sandbox-builder.md` (relates)
**Branch**: `linux-sandbox-app-daemon-client` (stacked on `unprivileged-cgroups-sandbox`)

## Checklist

- [x] Create changeset
- [x] Extend `StartSessionRequest` (`repo_path`, `claude_args`) + `MintLocalToken` RPC
- [x] Pure contracts: `local_identity_for_uid`, `local_token_login_for_uid`, `session_worktree_source`, `start_session_request_from` (unit-tested)
- [x] Step 1 — tonic codegen + adapter for `ConnectionService`
- [x] Step 2 — UDS `tonic::transport::Server` + SO_PEERCRED peer-trust + `MintLocalToken` (integration-tested)
- [x] Step 3 — daemon honors `repo_path`/`claude_args`; external checkout never auto-removed
- [x] Step 4 — app Linux client loop + terminal proxy (macOS path untouched)
- [x] Fix pre-existing debt (fmt drift; `type_complexity` in `cursor_cli_hooks_acceptance.rs`)
- [ ] **On-host end-to-end run (NOT done — see Validation)**
- [ ] **macOS build/clippy (NOT done — no darwin target in this env)**

## Motivation

On Linux an unprivileged `tddy-sandbox-app` cannot spawn the cgroups jail in-process (cgroup v2
delegation containment — it can't migrate its child into a limited scope whose common ancestor it
doesn't own). So on Linux the app delegates to a running `tddy-daemon`: it sends its config/params,
the daemon spawns the sandboxed session (it owns cgroup delegation + the AppArmor userns grant), and
the app PTY-proxies the terminal. Transport is native tonic gRPC over the daemon's Unix socket, with
SO_PEERCRED peer-trust auth. macOS keeps its in-process Seatbelt path.

## Files to create

| File | Purpose |
|------|---------|
| `packages/tddy-daemon/src/connection_tonic_adapter.rs` | Serve the existing `ConnectionServiceImpl` over the generated tonic trait; tonic-layer `mint_local_token` (peer-cred) |
| `packages/tddy-daemon/src/local_socket_server.rs` | Bind + serve the ConnectionService over a UDS `tonic::transport::Server` |
| `packages/tddy-daemon/tests/local_token_uds.rs` | Integration test: tonic client over UDS → `MintLocalToken` (mapped/unmapped peer) |
| `packages/tddy-sandbox-app/src/daemon_client.rs` | Linux flow: connect UDS → mint → StartSession → `StreamSessionTerminalIO` proxy |
| `packages/tddy-sandbox-app/src/codebase_mode.rs` | `resolve_codebase_mode` relocated (platform-agnostic) |

## Files to modify

| File | Change |
|------|--------|
| `packages/tddy-service/proto/connection.proto` | `StartSessionRequest.{repo_path,claude_args}`; `MintLocalToken` rpc + messages |
| `packages/tddy-service/build.rs`, `src/lib.rs` | tonic codegen pass for `connection.proto` (`.extern_path(".connection", …)`) + `tonic_connection` module |
| `packages/tddy-codegen/src/generator.rs` | `from_arc(Arc<T>)` constructor on generated `*Server` (share one impl across transports) |
| `packages/tddy-daemon/src/config.rs` | `local_identity_for_uid`, `local_token_login_for_uid`, `LocalConfig`/`local_socket_path()` |
| `packages/tddy-daemon/src/connection_service.rs` | tonic `mint_local_token` stub (unauth on non-UDS); `WorktreeSource`/`session_worktree_source`; `repo_path`/`claude_args`/`initial_prompt` threading; `sandbox_claude_passthrough_args` |
| `packages/tddy-daemon/src/session_deletion.rs` | `is_daemon_managed_worktree` guard — never remove a client's external checkout |
| `packages/tddy-daemon/src/{main.rs,lib.rs,user_sessions_path.rs}` | spawn UDS server; `username_for_uid` (getpwuid_r) |
| `packages/tddy-sandbox-runner/src/{runner.rs,lib.rs}` | extract + export `connect_uds_channel` (reused by the app client) |
| `packages/tddy-sandbox-app/src/{main.rs,bridge.rs,spawn.rs,config.rs,Cargo.toml}` | Linux/macOS `#[cfg]` split; darwin dep macOS-gated; `--daemon-socket` flag |
| `packages/tddy-daemon/tests/*_acceptance.rs` (10 files) | `..Default::default()` on `StartSessionRequest` literals (additive-field compile fix) |
| `packages/tddy-daemon/tests/cursor_cli_hooks_acceptance.rs`, `sandbox_session.rs`, `tddy-sandbox-recipes/{cursor_cli.rs,lib.rs}` | **pre-existing** debt fixes: `type_complexity` → `SessionUserResolver`; fmt |

## Design decisions

### App delegates to the daemon; tonic over the daemon's UDS
The daemon owns cgroup delegation + the userns grant, so it does the privileged spawn; the app is a
local terminal client. Native tonic (reusing the sandbox crate's UDS connector `connect_uds_channel`)
over the daemon's Unix socket — no ConnectRPC envelope hand-framing.

### Peer-trust auth (SO_PEERCRED), reusing session tokens
tonic 0.12's native `UdsConnectInfo` yields the peer uid; `MintLocalToken` maps it
(`local_token_login_for_uid` + `getpwuid_r`) → a signed access token. All other handlers keep their
existing `session_token` auth unchanged (mint is the only new auth surface, UDS-only).

### Serve the existing impl over tonic via a hand-written adapter
The codegen `generate_tonic_adapter` flag is not tonic-servable; the repo pattern is a hand-written
impl of the generated tonic trait. `ConnectionServiceTonicAdapter<T>` delegates all methods to the
existing `ConnectionServiceImpl` (shared via `from_arc`), overriding only `mint_local_token` at the
tonic layer (peer-cred).

### `repo_path` used directly, never destroyed
`session_worktree_source` branches: `Project` = existing git-worktree behavior byte-for-byte;
`RepoPath` = use the checkout directly (canonicalize + `is_dir`, no worktree, not daemon-managed).
`is_daemon_managed_worktree` (the `.worktrees` invariant) gates session-delete removal so a client's
checkout is never deleted. Runs as the caller's mapped os_user → no privilege beyond that user.

## Unit/integration tests

Pure unit: `local_identity_for_uid`, `local_token_login_for_uid`, `session_worktree_source`,
`sandbox_claude_passthrough_args`, `is_daemon_managed_worktree`, `start_session_request_from`,
daemon-socket-path resolution, terminal-input frame builders, specialized-agent forwarding.
Integration: `local_token_uds.rs` (mint for mapped peer; deny unmapped) + deletion-preservation test.
The connect→mint→start→stream loop is host-touching — verified manually on-host, not in CI.

## Validation Results

- **fmt**: `cargo fmt --check` clean (workspace) — includes fixing pre-existing drift.
- **clippy** (`--all-targets -D warnings`, tddy-service/daemon/sandbox-app/codegen/runner): clean —
  includes fixing pre-existing `type_complexity` in `cursor_cli_hooks_acceptance.rs`.
- **Tests**: all feature-package test binaries pass (tddy-daemon lib 278 + integration incl.
  `local_token_uds` 2; tddy-sandbox-app 32; tddy-service 38; tddy-sandbox-runner all) — 0 failures
  across two full runs.
- **Build**: full workspace + `tddy-sandbox-app` (pulls daemon/service/runner) compile clean.

### ⚠️ NOT verified (must happen before merge)
- **End-to-end has never been run.** Every automated test is pure/unit/transport-integration; the
  actual app↔daemon connect → mint → StartSession → live sandboxed terminal has not been executed.
  It requires the daemon running with the cgroups+AppArmor setup — whose own sandbox spawn still had
  open issues (controller EBUSY / jail) in the base branch `unprivileged-cgroups-sandbox` (PR #290).
- **macOS not built** (no darwin target here). The `#[cfg(macos)]` path is relocated-verbatim/unchanged,
  but needs a `cargo build`/`clippy` on macOS.
- **Stacks on the unmerged PR #290** — must land after it.
- **Intermittent flake**: `claude_cli_session_enrichment_reads_from_metadata` failed once under heavy
  parallel load (not caused by this change; `list_sessions` enrichment on the shared blocking pool
  under contention). Follow-up: `#[serial]` or decouple the list read from the timeout pool.

## Out of scope / follow-ups
- Cursor agent over the daemon path; resume re-threading `claude_args`/`initial_prompt`; a git-repo
  precondition on `repo_path`; harden the enrichment-test flake.
