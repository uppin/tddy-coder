# 2026-09-19 — Sandboxed codebase placement, from the web

**Type:** Feature

Adds a third codebase placement across `tddy-service`, `tddy-session-lifecycle`, `tddy-daemon`,
`tddy-daemon-livekit`, `tddy-daemon-sandbox`, `tddy-coder` and `tddy-web`. Product detail:
[remote-managed-worktree.md](../../ft/daemon/remote-managed-worktree.md) § Sandboxed codebase
placement.

## What landed

- `bool sandboxed_codebase = 39` on `StartSessionRequest`; `CodebasePlacement::SandboxedCodebase`
  and `classify_placement(&PlacementRequest)` with six refusals; `colocated_jail_tool_env`;
  the local start/resume/delete paths; the capability on both self-description surfaces; the web
  control and its placement algebra.
- `exec_tool_route` is **unchanged** — the agent's MCP addresses the workspace session, so the
  existing predicate already routes into the jail.

## Sandbox-runner leaks

53 orphaned `tddy-sandbox-runner` processes were found alive on a dev machine, some for **8 days**,
across four worktrees — all reparented to `launchd`. Fixed: the unbounded `relay.await` in
`sandbox_runner_spawn_smoke` that hung `cargo test -p tddy-daemon-sandbox` and made that crate
locally untestable; `impl Drop` on `WorkspaceSandboxRegistry` and `SandboxSessionState`;
`SpawnedJail` RAII guards so a failed assertion cannot orphan a runner; and the strong `Arc` cycle
in `install_sandbox_rpc_bridge`, which stopped every `test_service` daemon from ever dropping its
registry.

## Measurements recorded at wrap

File-length gate, production lines (budget 500). None reached budget; all eight carry a
`packages/*/docs/code-issues/oversized-file-*.md` record with the designed seam:

| File | Lines |
|---|---|
| `tddy-coder/src/run.rs` | 2682 |
| `tddy-daemon-livekit/src/livekit_peer_discovery.rs` | 1565 |
| `tddy-daemon/src/runtime.rs` | 1479 |
| `tddy-session-lifecycle/.../svc_start_session_core.rs` | 908 |
| `tddy-session-lifecycle/.../session_coordinate_handlers.rs` | 818 |
| `tddy-session-lifecycle/src/split_session.rs` | 645 |
| `tddy-daemon-sandbox/src/workspace_tool_sandbox.rs` | 590 |
| `tddy-session-lifecycle/.../svc_spawn_split_agent.rs` | 502 |

Decomposed: `tddy-web/src/index.tsx` 534 → **250** (under budget);
`CreateSessionPane.tsx` 1412 → **752** across 20 modules, verified against 245 unchanged assertions.

**Complexity records on `master` that this change regressed** (from #507, not present on this
branch, so recorded here rather than reconciled): `start_session_core` 842 → **853**,
`delete_paired_codebase_session` 61 → **85**, `spawn_split_agent` 213 → **233**.

## Backlog

`2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md` is **narrowed, not resolved**:
the daemon shutdown path is fixed, but `tddy-desktop` still orphans its jails
(`src-tauri/src/lib.rs:300`) and the crash-detector/restart-policy half is untouched.
