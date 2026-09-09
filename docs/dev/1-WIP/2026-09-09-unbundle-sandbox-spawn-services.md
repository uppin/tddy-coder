# Changeset: the sandbox, spawn and leaf RPC subsystems in their own crates

**Date**: 2026-09-09
**Status**: 🚧 In Progress
**Type**: Refactor
**Stack**: `#unbundle` node **3 of 8**. Base: `feature/unbundle/model-telegram-screen` (node 2)

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
| `tddy-actions` *(exists)* | `action_service.rs` | 329 | `action_service_acceptance.rs` |
| `tddy-task` *(exists)* | `task_service.rs` | 322 | `task_service_acceptance.rs` (877 LoC) |
| `tddy-bsp` *(exists)* | `bsp_service.rs` | 177 | its BSP suites |
| `tddy-semantic-index` *(exists)* | `semantic_index.rs` | 68 | `semantic_index_wiring.rs` |

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

- [ ] **`tddy-daemon-sandbox`**: crate, 5 modules, 16 test suites, the 6 `tddy-sandbox*` deps
- [ ] **`tddy-spawn`**: crate, 4 modules, 5 test suites, the `tddy-supervisor` dep
- [ ] **Leaf services**: `action_service`, `task_service`, `bsp_service`, `semantic_index` into their owners
- [ ] **`tool_catalog_sync.rs`** relocated to `tddy-daemon-sandbox/tests/`; `lib.rs:93` removed
- [ ] **`tddy-sandbox-app` reversal**: depends on `tddy-daemon-sandbox`, not `tddy-daemon`
- [ ] **⚠ nextest exclusions**: every `[profile.ci]` path whose file moved is updated in the same commit
- [ ] **Desktop**: `spawn_worker`/`supervisor_client` callers migrated; built locally
- [ ] **File budget**: record which over-500-line files landed under budget and which did not, with why
- [ ] **Baseline**: `./test` per touched package back to the recorded numbers
- [ ] **Code Quality**: `cargo clippy -p <each> -- -D warnings` clean, `cargo fmt` clean
- [ ] **Documentation**: doc triage executed at wrap

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

- [ ] M1 — the four leaf services move to their owner crates; their suites pass there
- [ ] M2 — `tddy-spawn` extracted; 5 suites pass; `tddy-supervisor` gone from `tddy-daemon`
- [ ] M3 — `tddy-daemon-sandbox` extracted; 16 suites pass; the 6 `tddy-sandbox*` deps moved
- [ ] M4 — `tool_catalog_sync.rs` is a test file in the sandbox crate
- [ ] M5 — `tddy-sandbox-app` depends on `tddy-daemon-sandbox`; asserted, not just described
- [ ] M6 — nextest `[profile.ci]` exclusion paths updated; the excluded set is unchanged in content
- [ ] M7 — desktop built locally; baselines restored; file-budget outcome recorded

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
- [ ] **Integration**: a Seatbelt-jailed session starts, serves tool IPC and exits (`sandboxed_claude_cli_acceptance.rs`)
- [ ] **Integration**: the workspace tool sandbox provisions and resumes (`workspace_tool_sandbox_acceptance.rs`, `workspace_sandbox_resume_acceptance.rs`)
- [ ] **Unit**: the plan builder composes a plan without `tddy-daemon` on the dependency path (`workspace_tool_sandbox_plan_unit.rs`)
- [ ] **Unit**: workspace exec tool names match the tool catalog, from `tests/` rather than `src/` (`tool_catalog_sync.rs`)

### tddy-spawn
- [ ] **Integration**: a spawn is delegated to the supervisor and the child reports back (`supervisor_spawn_delegation.rs`)
- [ ] **Unit**: spawn argv is composed from an agent def without `tddy-daemon` on the path (`agent_def_spawn_argv_unit.rs`)

### tddy-actions / tddy-task / tddy-bsp / tddy-semantic-index
- [ ] **Integration**: each service answers from its owner crate (`action_service_acceptance.rs`, `task_service_acceptance.rs`, the BSP suites, `semantic_index_wiring.rs`)

### tddy-sandbox-app
- [ ] **Integration**: the sandbox path resolves through `tddy-daemon-sandbox`, and `tddy-daemon` is absent from its dependency graph for it (`daemon_sandbox_dependency_acceptance.rs`)

### tddy-daemon
- [ ] **Integration**: the registered service names still include all four leaf services (`service_registration_acceptance.rs`)

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

- [ ] `2026-07-01-tddy-daemon.md`'s stdio-transport switch becomes easier after this node but is not
      done; re-read at wrap
- [ ] `tddy-desktop` is outside the CI gate and its `spawn_worker`/`supervisor_client` callers move
      here; the local build result is stated rather than assumed

## Baseline

| Gate | Before | After |
|---|---|---|
| `./test -p tddy-daemon` | | |
| `cargo clippy -p tddy-daemon -- -D warnings` | | |
| `cargo nextest run --profile ci -p tddy-daemon` (excluded-set audit) | | |

The known pre-existing failure inherited from node 1's baseline
(`cursor_cli_session_acceptance::…self_arc called before set_self_handle`) is expected to stay at
exactly one. Note that the five `sandbox_behavior_acceptance` failures known to fail on `master`
share its root cause and move to `tddy-daemon-sandbox` with the suite — they must still fail for the
same reason and must not be counted as this node's regressions.

## Final Checklist

- [ ] `docs/dev/changesets/2026-09-09-unbundle-sandbox-spawn-services.md` — the release-note file,
      carrying the `tddy-sandbox-app` dependency reversal and the exclusion-path audit result
- [ ] New `docs/dev/todo/` entry if any nextest exclusion turned out to be unnecessary after the move
- [ ] Re-read `docs/dev/todo/2026-07-01-tddy-daemon.md` against the new sandbox crate
- [ ] Doc triage: `grep -rn -e 'sandbox_session' -e 'spawner' -e 'spawn_worker' -e 'action_service' -e 'task_service' -e 'bsp_service' packages/tddy-daemon/README.md packages/tddy-daemon/docs docs/ft/daemon docs/dev/guides/ci.md`
