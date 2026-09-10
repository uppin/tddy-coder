# Changeset: the sandbox, spawn and leaf RPC subsystems in their own crates

**Date**: 2026-09-09
**Status**: 🚧 In Progress
**Type**: Refactor
**Stack**: `#unbundle` node **3 of 8**. PR [#472](https://github.com/uppin/tddy-coder/pull/472).
Base: `feature/unbundle/model-telegram-screen` (node 2, PR #471)

## Initial Discovery

Full codebase exploration that grounded this plan:
[2026-09-09-unbundle-sandbox-spawn-services-initial-discovery.md](./2026-09-09-unbundle-sandbox-spawn-services-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or item dumps here.

## Responsibility

Two subsystems become crates, and four leaf RPC services go home to the crates that already own their
domain. No proto changes; no client migrates.

| Destination | What moves | prod LoC | Dedicated tests |
|---|---|---:|---|
| `tddy-daemon-sandbox` *(new)* | `sandbox_session.rs`, `workspace_tool_sandbox.rs`, `sandbox_action.rs`, `sandbox_plan_builder.rs`, `sandbox_runtime.rs` | 2,202 | 16 files / 5,494 LoC |
| `tddy-spawn` *(new)* | `spawner.rs`, `spawn_worker.rs`, `supervisor_spawn.rs`, `supervisor_client.rs` | 3,236 | 5 files / 728 LoC |
| `tddy-bsp` *(exists)* | `bsp_service.rs` | 177 | its BSP suites |
| `tddy-semantic-index` *(exists)* | `semantic_index.rs` (helpers only, **no service**) | 68 | `semantic_index_wiring.rs` |
| ~~`tddy-actions`~~ | ~~`action_service.rs`~~ — **blocked, stays in `tddy-daemon`** | 329 | suite stays |
| ~~`tddy-task`~~ | ~~`task_service.rs`~~ — **blocked, stays in `tddy-daemon`** | 322 | suite stays |

### ⚠ Three of the four leaf services are not movable — corrected during implementation

The plan assumed the four leaf services were symmetric. They are not, and only `bsp_service`
could move. The blocking edge is **`tddy-service` → `tddy-core` → `tddy-task`**: any crate that
must import `tddy_service::proto::*` and is also an ancestor of `tddy-task` cannot exist.

| Planned move | Outcome | Why |
|---|---|---|
| `bsp_service` → `tddy-bsp` | ✅ done | `tddy-bsp` is a leaf — only `tddy-coder` and `tddy-daemon` depend on it — so it may depend on `tddy-service` |
| `task_service` → `tddy-task` | ❌ blocked | needs `tddy_service::proto::tasks::*`; cargo reports `cyclic package dependency: tddy-core depends on itself` |
| `action_service` → `tddy-actions` | ❌ blocked | `tddy-actions` → `tddy-task`, the same cycle; **and** it needs `crate::sandbox_runtime`, which does not leave `tddy-daemon` until M3 |
| `semantic_index` service entry | ❌ never existed | `semantic_index.rs` is three helpers (`semantic_index_db_path`, `semantic_index_env`, `run_semantic_index_blocking`). There is no `semantic_index.SemanticIndexService` in the proto and no server type. The module still moved; only the invented entry is dropped |

Cutting `tddy-core → tddy-task` would restructure `tddy-core`'s `session_actions` and
`session_catalog` (5 call sites) and is ruled out by `## Boundaries` below, which assigns every
cycle cut to node 1. Node 1's own cycle-cut commit records that four cross-crate cycles remain and
"belong to later nodes" — so **which node owns this edge is unresolved**, and it is recorded here
rather than decided unilaterally. The three `build_*_entry` stubs and the tests asserting them were
removed: they specified a shape the crate graph forbids.

**`tool_catalog_sync.rs` stops being a source file.** Its entire body is one `#[cfg(test)] mod tests`
with a single test (`workspace_exec_tool_names_match_tool_catalog`) — a test file that has been living
in `src/` and declared in `lib.rs:93`. It moves to `packages/tddy-daemon-sandbox/tests/`, which is
where a test asserting the workspace exec tool names belongs.

**`tddy-sandbox-app`'s dependency reverses.** It consumes
`tddy_daemon::{sandbox_session, claude_cli_session, tool_engine}` today; after this node it depends
on `tddy-daemon-sandbox` and not on `tddy-daemon` at all for the sandbox half. That reversal is the
node's most checkable outcome, and it is asserted rather than described.

## Boundaries

This PR explicitly does **not**:

- Change any proto, or touch `connection.ConnectionService`. Families B, I–N and P–T belong to nodes
  4 and 6–8; families E–H were node 1's.
- Move `tool_call_log.rs` or `session_toolcall.rs`. They are cohesive (494 LoC, zero outbound edges)
  but belong with tool *execution*, which is node 8.
- Move `pty_runtime.rs` or `terminal_session_adapter.rs`. Terminals are node 6.
- Collapse the hand-copied exec-tool catalog. `tddy-tools`' `server::exec_tool_catalog()` is a
  verbatim clone of `tddy_tool_engine::catalog::tool_catalog()`, guarded by matched tests in both
  crates — **collapsing it is node 8's**, where `tddy-tool-engine` gains the service. This node only
  relocates the daemon-side guard test.
- Move `daemon_settings.rs`, `daemon_config_service.rs`, `user_sessions_path.rs`,
  `tddy_user_config.rs`, `agent_list_mapping.rs` or `relay_idle.rs`. These are wiring-adjacent —
  `daemon_config.DaemonConfigService` serves the daemon's own configuration — and stay deliberately.
- Implement `move_module_to_crate`, `tddy-daemon-kernel`'s symbols, or any cycle cut. All node 1's.
- Force every file under 500 lines. `spawner.rs` (2,296), `sandbox_session.rs` (1,017),
  `daemon_settings.rs` (550, staying) and `spawn_worker.rs` (527) are over budget and are split where
  the seams are cohesive; whatever stays over is recorded in `## Scope`.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` host-worktree-services | `move_module_to_crate` in `tddy-code-restructuring` | every move here is a plan the operation executes | add, extend or fix the operation |
| `n1` host-worktree-services | `tddy-daemon-kernel` exporting **`AgentActivityHub`** and **`now_unix_ms()`** | `sandbox_session.rs` holds `Arc<AgentActivityHub>` at `:197` and reads it at `:225,262,364`, and calls `now_unix_ms` at `:999,1025`. These are the **only** things the sandbox subsystem reaches into `connection_service` for, so the kernel is exactly what unblocks this node | define, re-export or re-implement `AgentActivityHub` or `now_unix_ms`; a change needed to either is reported upward |
| `n1` host-worktree-services | `run_server(RunServerOptions)` | four service registrations leave that struct's argument list; this PR edits its fields, not its shape | change the options struct's shape |
| `n2` model-telegram-screen | nothing this PR consumes | — | — |

`n2` is this PR's **branch** parent but not a dependency: the two nodes touch disjoint subsystems. The
only file they share is `packages/tddy-daemon/src/runtime.rs`, where each removes its own registrations.

## Draft PR contract

What lands in this PR's **second commit**:

- `packages/tddy-daemon-sandbox/src/lib.rs` and `packages/tddy-spawn/src/lib.rs` declaring each
  crate's public surface with real signatures — the `WorkspaceSandbox` and
  `WorkspaceSandboxProvisioner` trait ports, `SandboxSessionManager`, `SpawnClient`, and
  `spawn_backend_choice`/`spawn_worker_for` — bodies annotated `// TODO(sandbox-spawn-services): implement`.
- The `build_*_entry(...) -> ServiceEntry` signatures added to `tddy-actions`, `tddy-task`,
  `tddy-bsp` and `tddy-semantic-index`.
- Both new `Cargo.toml`s and their workspace `members` entries.
- The failing acceptance tests, including the one that asserts `tddy-sandbox-app` no longer depends on
  `tddy-daemon` for the sandbox path.

**This is the first push of a PR that goes on to implement the same thing. It must never merge in
that state.**

## Green wave

**Wave:** 2 of 3
**Greenable independently:** yes — once node 1 is green, because `AgentActivityHub` and `now_unix_ms`
are the only cross-subsystem surfaces this node's tests touch, and both are node 1's published
kernel. Every other test mounts this node's own crate or injects a double.
**Concurrent with:** nodes 2, 4 and 5 — disjoint subsystems, protos and test files.
**Blocks:** nothing. Node 8 collapses a duplication this node deliberately leaves alone, but it
consumes no surface from here.

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4, n5      n5 → n6, n7, n8

⚠ **Recurring conflict**: `packages/tddy-daemon/src/runtime.rs` is edited by nodes 2–5 and 6–8.
Every cascade will conflict there. Resolve by keeping every node's removals.

## Affected Packages

- **tddy-daemon-sandbox** *(new)* — the 5 sandbox modules; the 6 `tddy-sandbox*` dependencies move here
- **tddy-spawn** *(new)* — the 4 spawn/supervisor modules; `tddy-supervisor` moves here
- **tddy-actions**, **tddy-task**, **tddy-bsp**, **tddy-semantic-index** *(exist)* — each gains the
  RPC service for the domain it already owns, plus a `clap`-free entry constructor
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md) — 13 modules leave, 1 becomes a
  test file
- **tddy-sandbox-app**: its `tddy-daemon` dependency **reverses** to `tddy-daemon-sandbox`
- **tddy-service**: no proto change
- **tddy-desktop**: consumes `spawn_worker` and `supervisor_client`, both of which move here — so
  `src-tauri/src/lib.rs` changes. **Outside the CI gate**: built locally and stated

## Related Feature Documentation

**None — behaviour-preserving restructure. No PRD.** No RPC coordinate moves, no client migrates, no
observable behaviour change. Following the precedent of
`docs/dev/1-WIP/2026-09-09-connection-service-split.md`.

## Summary

The sandbox and spawn subsystems become `tddy-daemon-sandbox` and `tddy-spawn`; four leaf RPC services
move into the crates that already own their domains; and `tool_catalog_sync.rs` stops pretending to be
a source file.

## Background

Discovery refuted the assumption that sandbox and spawn are one cluster. They are two, and they share
**no** `crate::` edge except `spawn_worker → spawner` and `supervisor_spawn → {spawn_worker, spawner,
supervisor_client}` — the sandbox modules never touch `spawner`. Confinement and privileged fork are
different concerns with different dependency sets (`tddy-sandbox*` ×6 versus `tddy-supervisor`), so
they become two crates in one node rather than one crate.

The sandbox subsystem has the **best test locality in the daemon**: 62 inline test LoC against 5,494
LoC of dedicated integration suites. That is the strongest possible position from which to move code —
the tests that prove behaviour move with it, unrewritten.

The four leaf services are each ~70–330 lines of RPC surface over a domain that already has a crate.
`action_service.rs` wraps `tddy-actions`, `task_service.rs` wraps `tddy-task`, `bsp_service.rs` wraps
`tddy-bsp`, `semantic_index.rs` wraps `tddy-semantic-index`. Each is a `ServiceEntry` constructor that
belongs beside the thing it exposes.

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ⚠ DURING — `2026-08-13-deterministic-test-suite-deliberate-gaps.md` and `2026-08-02-unprivileged-userns-available-under-approximates-what-the-jail-needs.md`

Both concern the sandbox suites' environment sensitivity. Several move to a new crate here, and the
move must not change which of them CI runs: the `.config/nextest.toml` `[profile.ci]` exclusions name
suites by path, so **every exclusion whose path changes must be updated in the same commit** or an
excluded test silently starts running (or a running one silently stops). This is the one way this
node can break the gate invisibly, and it earns a `## Scope` line.

### ⚠ DURING — `2026-08-15-echoes-a-message-over-sandbox-service-served-over-stdio-is-skipped-in.md`

`sandbox_runner_stdio_acceptance::echoes_a_message_over_sandbox_service_served_over_stdio` is
explicitly excluded from CI and documented as such in `docs/dev/guides/ci.md`. It moves with the
sandbox crate; its exclusion path must move too. Recorded, not fixed here.

### ⚠ DURING — `2026-07-01-tddy-daemon.md`

Records that the daemon's real session lifecycle should switch onto the stdio transport, and that the
remaining work is *"purely wiring the daemon's own spawn/dial call sites in `connection_service.rs`"*
— deferred because that file's orchestration was large. This node moves `sandbox_session.rs`'s
`dial_and_bridge` into its own crate, which makes that switch materially easier later. It is **not**
done here: it changes live transport behaviour for every real session, which is not a relocation.
Re-read the entry at wrap.

### ⚠ DURING — `2026-08-03-tddy-supervisor-deliberate-gaps-and-follow-ups.md` and `2026-08-02-tddy-supervisor-follow-ups.md`

Open items in the spawn/supervisor path. They move unchanged; recorded so a reviewer seeing them in
`tddy-spawn` knows they are inherited.

## Scope

- [x] **`tddy-daemon-sandbox`**: crate, 5 modules, **5** of 16 test suites, **2** of the 6
      `tddy-sandbox*` deps — ⚠ not the counts the plan assumed, see M3 below
- [x] **`tddy-spawn`**: crate, 4 modules, 4 suites moved, `tddy-supervisor` out of `[dependencies]`
      — ⚠ it remains a **`[dev-dependencies]`** entry, see M2 below
- [x] **Leaf services**: `bsp_service` → `tddy-bsp` ✅. `action_service` and `task_service` **stay in
      `tddy-daemon`** (cycle, see above); the `semantic_index` service entry was dropped as a false
      premise, though the module itself moved
- [x] **`tool_catalog_sync.rs`** relocated to `tddy-daemon-sandbox/tests/`; the `lib.rs` declaration removed
- [x] **`tddy-sandbox-app` reversal**: depends on `tddy-daemon-sandbox`, not `tddy-daemon`
- [x] **⚠ nextest exclusions**: one of the five in-scope predicates changed package and was repointed;
      the excluded set is identical in content
- [x] **Desktop**: `spawn_worker`/`supervisor_client` callers migrated to `tddy_spawn::` and
      `tddy-desktop` **built locally** — `cargo build -p tddy-desktop` clean in 4m02s. This is the
      one break CI cannot catch: `tddy-desktop` is outside the gate ✅
- [x] **File budget**: recorded — `spawner.rs` (2,152), `spawn_worker.rs` (568) and
      `workspace_tool_sandbox.rs` (521) stayed over budget, moved unsplit so the rename-similarity
      evidence survives. See [`../todo/2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md`](../todo/2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md) ✅
- [~] **Baseline**: scoped per-package runs green (`tddy-daemon-sandbox` 27/0, `tddy-spawn` 45/0, the
      four leaf-owner crates 0 failures). A full `tddy-daemon` suite could **not** be completed on this
      host — it ran out of disk, then out of memory — so that number is CI's, per CLAUDE.md
- [x] **Code Quality**: `cargo clippy -p <each touched> --all-targets -- -D warnings` clean,
      `cargo fmt` clean; CI's workspace-wide `Rust lint` also passed ✅
- [~] **Documentation**: doc triage executed — `docs/ft/daemon/background-tasks.md` verified still
      correct (both services it lists stayed), one stale `spawner.rs` path fixed in
      `remote-managed-worktree.md`. `packages/tddy-daemon/docs/connection-service.md` still names
      moved modules and is **left to the wrap step**: CLAUDE.md forbids editing `packages/*/docs/`
      directly

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical Changes

### State A

| | Files | prod LoC | inline tests | Reaches `connection_service` for |
|---|---:|---:|---:|---|
| sandbox | 6 | 2,171 | **62** | `AgentActivityHub` (5 sites), `now_unix_ms` (2) — nothing else |
| spawn/supervisor | 4 | 2,769 | 467 | nothing |
| leaf services | 4 | 896 | ~300 | nothing (all `runtime`-only in-edges) |

`tool_catalog_sync.rs` is 31 lines, all of it `#[cfg(test)]`. `tddy-sandbox-app` depends on
`tddy-daemon`. Internal sandbox chain: `sandbox_runtime → sandbox_action → sandbox_plan_builder →
sandbox_session`. Internal spawn chain: `supervisor_spawn → {spawn_worker, spawner,
supervisor_client}`; `spawn_worker → spawner`.

### State B

`tddy-daemon` loses 13 modules and 5,836 prod LoC. `tddy-daemon-sandbox` and `tddy-spawn` exist;
`tddy-actions`, `tddy-task`, `tddy-bsp` and `tddy-semantic-index` each expose a
`build_*_entry(...) -> tddy_rpc::ServiceEntry`. `tddy-sandbox-app` depends on `tddy-daemon-sandbox`.
`runtime.rs` registers four services through their owners' constructors.

### Delta

#### tddy-daemon
- **Architecture**: 13 modules leave; `tool_catalog_sync` becomes a test; `lib.rs` loses 14 entries
- **Implementation**: `runtime.rs`'s `actions.ActionService`, `tasks.TaskService`, `bsp.BspService`
  and semantic-index registrations move behind their owners' constructors; the sandbox provisioner and
  the pre-tokio spawn-worker fork are called from the new crates
- **Dependencies**: the 6 `tddy-sandbox*` crates and `tddy-supervisor` leave

#### tddy-daemon-sandbox, tddy-spawn
- **Architecture**: new crates, workspace members
- **API**: the existing trait ports unchanged in shape; `AgentActivityHub` imported from the kernel

#### tddy-actions / tddy-task / tddy-bsp / tddy-semantic-index
- **API**: one entry constructor each. `tddy-bsp` already owns `plugins::plugin_registry`, so its
  service lands beside the registry it uses

#### tddy-sandbox-app
- **Dependencies**: `tddy-daemon` → `tddy-daemon-sandbox` for the sandbox path

## Implementation Milestones

- [~] M1 — **rescoped**: `bsp_service` moved to `tddy-bsp` and its suites pass there;
      `semantic_index.rs` moved to `tddy-semantic-index`. `action_service` and `task_service` are
      blocked by the `tddy-core → tddy-task` cycle and stay in `tddy-daemon`
- [x] M2 — `tddy-spawn` extracted; 45 tests pass in the new crate; `tddy-supervisor` out of
      `tddy-daemon`'s `[dependencies]`. ⚠ **Not literally complete**: see below

### M2 outcome — three corrections to the plan

**1. `DaemonConfig` never needed decoupling.** The extraction was expected to cost a signature
change at 7 `spawn_backend_choice(&DaemonConfig)` call sites, on the assumption that `DaemonConfig`
was daemon-owned. It is not: it is defined at `packages/tddy-daemon-kernel/src/config.rs:244` and
merely re-exported by `packages/tddy-daemon/src/lib.rs:22`. `tddy-spawn` already depends on the
kernel for `spawn_as_user`, so it reads the type directly. Likewise `tddy_user_config.rs` is only a
re-export shim over `tddy_daemon_kernel::user_paths` and stays put, untouched, as `## Boundaries`
requires. **No signature changed and no suite was rewritten** — which is what let
`supervisor_routing.rs` move with its `spawn_backend_choice(&config)` calls intact.

**2. The red-phase surface in `tddy-spawn/src/lib.rs` was mis-specified and is deleted.** It invented
`SpawnBackend`/`SpawnClient`/`SpawnError` and three tests. One of them,
`refuses_to_fall_back_when_a_configured_supervisor_is_unreachable`, demanded
`spawn_worker_for(Supervisor)` return `Err` — the direct opposite of
`supervisor_spawn_delegation.rs:337`, an existing passing suite that asserts *"deciding not to fork
cannot fail"*. `Ok(None)` is correct: a supervised daemon forks nothing because the supervisor
spawns, and reachability is `connect_supervisor`'s job, which already fails hard. Implementing it
would have made every supervised daemon fail to start. Its enum also dropped the `socket_path` all 7
call sites need. The real `SpawnBackendChoice` / `spawn_backend_choice` / `spawn_worker_for` arrived
with `supervisor_client.rs` instead. Same precedent as the three M1 stubs dropped above.

**3. `move_module_to_crate` cannot express this move.** `restructure check` passed on a 4-op plan,
but the operation (a) re-points `crate::` at the *source* crate, so `crate::config` becomes
`tddy_daemon::config` — a `tddy-spawn → tddy-daemon` cycle; and (b) moves one module at a time, so
moving `spawner` first rewrites its three siblings' `crate::spawner` to `tddy_spawn::spawner` before
they themselves have moved. The four modules are mutually entangled and must land together. Moved by
hand with `git mv`, history preserved. Separately, `restructure apply --indexing-budget 900`
**did not honour the budget** — it failed with *"rust-analyzer had not finished indexing after 46s"*.
Both are node 1's surface; reported upward, not fixed here.

**What is not done:** `supervisor_spawn_delegation.rs` stays in `tddy-daemon`. Nine of its 11 tests
are spawn-side, but two mount `ConnectionServiceImpl`, `multi_host`, `livekit_peer_discovery` and
`claude_cli_session`. Moving it whole would make `tddy-daemon` a dev-dependency of `tddy-spawn` — a
dev-graph cycle that rebuilds the entire daemon on `cargo test -p tddy-spawn`. So `tddy-supervisor`
survives in `tddy-daemon`'s `[dev-dependencies]`, and M2's *"`tddy-supervisor` gone from
`tddy-daemon`"* is met structurally but not literally. Splitting that suite's fail-closed half is a
clean follow-up.

**Deferred work recorded in `docs/dev/todo/`**, so it outlives this changeset's wrap:

- [`2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md`](../todo/2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md)
  — `spawner.rs` (2,152) and `spawn_worker.rs` (568) moved unsplit, with the three seams a later
  refactor should cut along, and why splitting during a relocation would have destroyed the
  rename-similarity evidence.
- [`2026-09-10-supervisor-spawn-delegation-keeps-tddy-supervisor-in-the-daemon.md`](../todo/2026-09-10-supervisor-spawn-delegation-keeps-tddy-supervisor-in-the-daemon.md)
  — the nine-plus-two test split that would let `tddy-supervisor` leave `[dev-dependencies]`.
- [`2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md`](../todo/2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md)
  — the two tooling defects, reported upward to node 1 rather than fixed here.

**One visibility widening**, the only one in the move: `spawner::resolve_livekit_room_name`
`pub(crate)` → `pub`, because three daemon call sites
(`telegram_session_control.rs:3170`, `connection_service.rs`, `terminal_bridge_impl.rs`) name the
same room the spawn does, and `pub(crate)` does not cross a crate boundary.
- [x] M3 — `tddy-daemon-sandbox` extracted; 27 tests pass in the new crate. ⚠ **Not literally
      complete**: 5 suites moved, not 16, and 2 sandbox deps left the daemon, not 6 — see below
- [x] M4 — `tool_catalog_sync.rs` is a test file in the sandbox crate
- [x] M5 — `tddy-sandbox-app` depends on `tddy-daemon-sandbox`; asserted, not just described
- [x] M6 — nextest `[profile.ci]` exclusion paths updated; the excluded set is unchanged in content
- [x] M7 — desktop built locally ✅; file-budget outcome recorded in `docs/dev/todo/` ✅; baselines
      are CI's to report, since the host could not complete a full `tddy-daemon` suite (disk, then OOM)

### M3–M6 outcome — four corrections to the plan

**1. Five of the sixteen sandbox suites moved, not sixteen.** The plan counted suites by subject
matter; the crate graph counts them by what they *mount*. Eight of the sixteen build a
`ConnectionServiceImpl` (`sandbox_behavior_acceptance`, `sandboxed_claude_cli_acceptance`,
`sandboxed_cursor_cli_acceptance`, `sandboxed_session_lifecycle_acceptance`,
`workspace_sandbox_resume_acceptance`, `workspace_tool_sandbox_acceptance`,
`workspace_tool_sandbox_seatbelt_acceptance`) or the two leaf services the `tddy-core → tddy-task`
cycle stranded in the daemon (`action_sandbox_acceptance` mounts `ActionServiceImpl` and
`TaskServiceImpl`). Moving any of them would make `tddy-daemon` a **dev-dependency of
`tddy-daemon-sandbox`** — the exact dev-graph cycle M2 refused for `supervisor_spawn_delegation.rs`,
and it would rebuild the whole daemon on `cargo test -p tddy-daemon-sandbox`.

One more stayed for a subtler reason: `sandbox_session_stdio_acceptance.rs`'s
`sandboxed_session_spawn_argv_carries_stdio_and_no_grpc_flags` does
`include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/connection_service.rs"))` — a **compile-time**
read of a daemon source file. Its other tests are pure sandbox, but the suite cannot leave the crate
that owns the file it greps. Splitting it is the same clean follow-up shape as M2's.

So the five that moved are the ones with no daemon surface at all: `sandbox_runner_inspect`,
`sandbox_runner_spawn_smoke`, `sandbox_runner_stdio_acceptance`, `sandbox_stdio_seatbelt_acceptance`
and `workspace_tool_sandbox_plan_unit`. They moved **unrewritten** — only `crate::`/`tddy_daemon::`
import prefixes changed, no assertion did. The `../../target/debug` fallbacks they use to find
`tddy-sandbox-runner` and `tddy-tools` resolve identically from the new manifest directory.

**2. Two `tddy-sandbox*` crates left the daemon, not six.** `tddy-sandbox-qemu` and
`tddy-sandbox-darwin` are now unreferenced anywhere in `tddy-daemon` and were deleted from its
manifest; `tddy-sandbox-cgroups` is referenced only by the four suites that stayed, so it moved from
`[target.'cfg(target_os = "linux")'.dependencies]` to the matching `dev-dependencies`. The other three
never belonged to the sandbox subsystem alone: `tddy-sandbox` is used by `split_session.rs` (8 files
in `src/`), `tddy-sandbox-recipes` by `split_session.rs`, and `tddy-sandbox-runner` by
`daemon_rpc_handler.rs` and `rpc_service.rs`. State A's "6 `tddy-sandbox*` dependencies move here"
counted the crates the subsystem *uses*, not the ones only it uses.

**3. The one real coupling was a drop-carrier, and is type-erased rather than moved.**
`SandboxSessionState` held `crate::session_toolcall::ManagedWorkflow`, which `## Boundaries` keeps in
`tddy-daemon` for node 8 — so importing it would have been a cycle. No method was ever called on it:
the field is `_managed_workflow`, and its own doc says it is "kept here so its lifetime is tied to
the session and its socket is cleaned up on drop". It is now
`Option<Box<dyn sandbox_session::SessionScopedResource>>`, a `Send + Sync` marker trait the daemon
implements for `ManagedWorkflow` in one line beside the struct. **Drop semantics are unchanged**:
dropping a `Box<dyn Trait>` runs the concrete type's drop glue through the vtable, so
`ManagedWorkflow`'s fields still drop in declaration order and `SessionToolcallListener::drop` still
aborts the accept task and unlinks the socket. `ManagedWorkflow` was not re-declared, re-implemented
or moved.

**4. The red-phase surface in `tddy-daemon-sandbox/src/lib.rs` was mis-specified and is deleted.**
Same shape as M2's correction 2. It invented a `SandboxSessionManager::new(Arc<AgentActivityHub>)`
with an `async start()` — but the real `SandboxSessionManager` is a registry of live sessions keyed
by `session_id` whose `new()` takes no arguments, called that way at
`connection_service/svc_resolve_tddy_tools_path.rs:106`; and it invented a `SandboxError` enum while
`tddy_sandbox::SandboxError` is the one all five modules already use. Its two tests asserted the
invented shape. Keeping either would have meant two `SandboxSessionManager`s and two `SandboxError`s
in one crate. `WorkspaceSandbox` and `WorkspaceSandboxProvisioner` were re-specifications of the real
traits in `workspace_tool_sandbox.rs`, which arrived intact with the move. `lib.rs` is now the crate
doc plus five `pub mod` lines.

**Also corrected:** `move_module_to_crate` was not attempted — the five modules are mutually
entangled exactly as M2's four were, and
[`2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md`](../todo/2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md)
already records why the operation cannot express that. Moved by hand with `git mv`, history
preserved.

**The nextest exclusion audit (M6).** Five `[profile.ci]` entries were in scope. Exactly one suite
changed package, and its predicate was repointed in the same change:

| Excluded entry | Suite lands in | Predicate |
|---|---|---|
| `sandboxed_cursor_cli_acceptance` | `tddy-daemon` | unchanged |
| `sandboxed_claude_cli_acceptance` | `tddy-daemon` | unchanged |
| `action_sandbox_acceptance::sandboxed_bash_action_writes_to_output_dir` | `tddy-daemon` | unchanged |
| `cursor_cli_session_acceptance::cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available` | `tddy-daemon` | unchanged |
| `sandbox_runner_stdio_acceptance::echoes_a_message_over_sandbox_service_served_over_stdio` | **`tddy-daemon-sandbox`** | `package(tddy-daemon)` → `package(tddy-daemon-sandbox)` |

The excluded set is **identical in content**, differing only in the package half of one predicate.
`docs/dev/guides/ci.md`'s fixture-binary table (which named `tddy-daemon` as the crate exec'ing
`target/debug/tddy-sandbox-runner`) and
[`2026-08-15-echoes-a-message-over-sandbox-service-served-over-stdio-is-skipped-in.md`](../todo/2026-08-15-echoes-a-message-over-sandbox-service-served-over-stdio-is-skipped-in.md)
were updated to the new path.

**No visibility widening was needed.** The five modules contain no `pub(crate)` items — every symbol
the daemon reaches was already `pub`, because they were `pub mod` in a `pub` lib.

**A second pre-existing `tddy-daemon` failure surfaced.** The plan's baseline records one
(`cursor_cli_session_acceptance::…self_arc called before set_self_handle`). Verifying the suite that
had to stay behind turned up another:
`sandbox_session_stdio_acceptance::sandboxed_session_spawn_argv_carries_stdio_and_no_grpc_flags`
`include_str!`s `connection_service.rs` and asserts it contains `"--stdio"`. PR #468 moved that argv
into three `svc_*` submodules, so the literal has been absent from the parent file since #468 landed
on `master` — confirmed against `master`, not this branch. Recorded in
[`2026-09-10-sandboxed-session-spawn-argv-greps-a-file-the-connection-service-split-emptied.md`](../todo/2026-09-10-sandboxed-session-spawn-argv-greps-a-file-the-connection-service-split-emptied.md);
not fixed here, because repointing a test's subject is not a relocation. It is also the single reason
that suite could not move.

**File budget:** `sandbox_session.rs` is **1,115** lines (1,100 before; +15 for the
`SessionScopedResource` trait and its doc). Over the 500-line budget, unsplit, for the same reason
M2 left `spawner.rs` unsplit — splitting during a relocation destroys the rename-similarity evidence
that proves the move was faithful. `workspace_tool_sandbox.rs` (521) is marginally over; the other
three are under.


## Testing Plan

**Primary test level: integration, per package.** 6,222 LoC of dedicated suites already prove this
behaviour and **move with the code** rather than being rewritten — the strongest available evidence
that nothing changed. The sandbox subsystem's 62-to-5,494 inline-to-integration ratio is what makes
this affordable.

Three proofs a test cannot carry:

- **`restructure verify --against <pre-move ref>`** from the repo root — statement multisets compared
  repo-wide, paths not compared, so a cross-crate move is validated directly.
- **The moved-line diff** — normalise `pub(crate)` and whitespace away, set-compare every moved line
  against the pre-move ref, state how many differ and why each one does. Alongside the visibility
  table, never instead of it.
- **The nextest exclusion audit** — diff `[profile.ci]`'s resolved test set before and after. It must
  be identical in content, differing only in path. This is the check that stops the gate weakening
  silently.

One assertion is new rather than moved: that `tddy-sandbox-app` reaches the sandbox through
`tddy-daemon-sandbox`, so the reversal cannot regress.

## Acceptance Tests

### tddy-daemon-sandbox
- [~] **Integration**: a Seatbelt-jailed session starts, serves tool IPC and exits — the suite
      **stayed in `tddy-daemon`**: it mounts `ConnectionServiceImpl`, so moving it would make
      `tddy-daemon` a dev-dependency of `tddy-daemon-sandbox`. Still passing where it is
- [~] **Integration**: the workspace tool sandbox provisions and resumes — both suites **stayed in
      `tddy-daemon`** for the same dev-graph-cycle reason
- [x] **Unit**: the plan builder composes a plan without `tddy-daemon` on the dependency path
      (`workspace_tool_sandbox_plan_unit.rs`, now in `tddy-daemon-sandbox/tests/`) ✅
- [x] **Unit**: workspace exec tool names match the tool catalog, from `tests/` rather than `src/`
      (`tool_catalog_sync.rs`) ✅

### tddy-spawn
- [~] **Integration**: a spawn is delegated to the supervisor and the child reports back — the suite
      **stayed in `tddy-daemon`**: 2 of its 11 tests mount `ConnectionServiceImpl`. See
      [`../todo/2026-09-10-supervisor-spawn-delegation-keeps-tddy-supervisor-in-the-daemon.md`](../todo/2026-09-10-supervisor-spawn-delegation-keeps-tddy-supervisor-in-the-daemon.md)
- [x] **Unit**: spawn argv is composed from an agent def without `tddy-daemon` on the path
      (`agent_def_spawn_argv_unit.rs`, now in `tddy-spawn/tests/`) ✅

### tddy-actions / tddy-task / tddy-bsp / tddy-semantic-index
- [~] **Integration**: each service answers from its owner crate — **only `bsp_service` and
      `semantic_index_wiring` moved.** `action_service` and `task_service` are blocked by the
      `tddy-core → tddy-task` cycle and stay in `tddy-daemon` with their suites (see the M1 correction above)

### tddy-sandbox-app
- [x] **Integration**: the sandbox path resolves through `tddy-daemon-sandbox`, and `tddy-daemon` is
      absent from `tddy-sandbox-app`'s manifest entirely ✅ — the suite landed as
      `sandbox_app_dependency_reverses.rs`, not the planned `daemon_sandbox_dependency_acceptance.rs`

### tddy-daemon
- [🔲] **Integration**: the registered service names still include all four leaf services —
      **`service_registration_acceptance.rs` was never written by the red phase and does not exist.**
      Only `bsp_service` moved, so the premise ("all four") no longer holds either

## Decisions & Trade-offs

- **Sandbox and spawn are two crates, not one.** Discovery refuted the grouping: they share no
  `crate::` edge, and their dependency sets are disjoint (`tddy-sandbox*` ×6 versus
  `tddy-supervisor`). One crate would have forced every sandbox consumer to link the supervisor.
- **The exec-tool catalog duplication is deliberately left standing.** `tddy-tools`'
  `exec_tool_catalog()` is a verbatim hand-copy of `tddy_tool_engine::catalog::tool_catalog()`,
  guarded by matched tests in both crates. Collapsing it belongs to node 8, where `tddy-tool-engine`
  gains the exec-tool service and becomes the single owner. Collapsing it here would leave the
  duplication half-resolved across two PRs.
- **`daemon_settings.rs` and `daemon_config_service.rs` stay.** They serve the daemon's *own*
  configuration over `daemon_config.DaemonConfigService`. That is wiring, not a subsystem, and the
  endpoint of this stack is a daemon that still owns its own configuration.
- **The nextest exclusion audit is treated as a deliverable, not a chore.** `[profile.ci]` names
  excluded tests by path, and this node moves several of them. A missed path either starts running an
  excluded test or stops running a covered one, and both are invisible in a green build. It gets its
  own `## Scope` line and its own check in the testing plan.
- **`tool_catalog_sync.rs` is relocated, not deleted.** It is a real test with a real assertion; what
  was wrong was its location. Deleting it would lose the guard that keeps the two catalogs in step —
  which node 8 then relies on.

## Technical Debt & Production Readiness

- [x] `2026-07-01-tddy-daemon.md`'s stdio-transport switch becomes easier after this node but is not
      done; re-read at wrap
- [x] `tddy-desktop` is outside the CI gate and its `spawn_worker`/`supervisor_client` callers moved
      here; **`cargo build -p tddy-desktop` passes locally** (stated, not assumed)

## Baseline

| Gate | Before | After |
|---|---|---|
| `./test -p tddy-daemon` | **1027 passed / 1 failed**, 25 suites (inherited from node 1) | |
| `cargo clippy -p <the 6 touched crates> --all-targets -- -D warnings` | ✅ exit 0 | |
| `cargo nextest run --profile ci -p tddy-daemon` (excluded-set audit) | not yet run — no suite has moved in commit 2 | |

**11 failing tests** define this node: 1 in `tddy-daemon-sandbox`, 3 in `tddy-spawn`, one per leaf
service in `tddy-actions`/`tddy-task`/`tddy-bsp`/`tddy-semantic-index`, and 3 asserting the
`tddy-sandbox-app` dependency reversal and the relocated catalog guard. The existing suites in the
owner crates still pass — 18 in `tddy-task`, 4 in `tddy-bsp`, 2 in `tddy-actions` — so the added
surface breaks nothing it landed beside.

The known pre-existing failure inherited from node 1's baseline
(`cursor_cli_session_acceptance::…self_arc called before set_self_handle`) is expected to stay at
exactly one. Note that the five `sandbox_behavior_acceptance` failures known to fail on `master`
share its root cause and move to `tddy-daemon-sandbox` with the suite — they must still fail for the
same reason and must not be counted as this node's regressions.

## Final Checklist

- [x] [`docs/dev/changesets/2026-09-10-unbundle-sandbox-spawn-services.md`](../changesets/2026-09-10-unbundle-sandbox-spawn-services.md)
      — the release-note file, carrying the `tddy-sandbox-app` dependency reversal, the exclusion
      audit result and the four declined moves ✅
- [x] New `docs/dev/todo/` entry if any nextest exclusion turned out to be unnecessary after the move —
      **none was.** All five in-scope exclusions are still needed; one changed package and was repointed ✅
- [x] Re-read `docs/dev/todo/2026-07-01-tddy-daemon.md` against the new sandbox crate ✅ — annotated
      there: `dial_and_bridge` now sits in `tddy-daemon-sandbox` while the spawn/dial call sites stay in
      `connection_service`, so the switch is better isolated but still a live-behaviour change, still open
- [x] Doc triage (executed 2026-09-10; see the **Documentation** line in `## Scope`): `grep -rn -e 'sandbox_session' -e 'spawner' -e 'spawn_worker' -e 'action_service' -e 'task_service' -e 'bsp_service' packages/tddy-daemon/README.md packages/tddy-daemon/docs docs/ft/daemon docs/dev/guides/ci.md`
