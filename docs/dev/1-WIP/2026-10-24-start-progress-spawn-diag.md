# Changeset: A stalled or impossible spawn says why

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — two crates (`tddy-daemon-kernel`, `tddy-spawn`), fork/exec ordering and async-signal safety, a spawned-process seam
**Stack**: #start-progress 1/8. PR: #734. Base: `master`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-24-start-progress-spawn-diag-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md](../todo/2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md) | ⚠ DURING | Same privilege drop. After this node a non-root probe fails **before forking** with a message naming both users; the deployment decision the entry asks for is node 3's supervisor handoff, not this node. Not resolved here. |
| [2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md](../todo/2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md) | ⚠ DURING | `spawner.rs` is 2,287 lines. The stage pipe and privilege drop go in `tddy-daemon-kernel/src/privilege_drop.rs` (+ a new sibling module); `spawner.rs` only loses code. |
| `packages/tddy-daemon-kernel/docs/code-issues/misplaced-tests-privilege-drop.md` | ⚠ DURING | Unclaimed. New symbols added to `privilege_drop` get their tests **in this crate**; the five existing tests are not moved here. |
| `packages/tddy-daemon-kernel/docs/code-issues/missing-tests-privilege-drop-resolve-pty-os-user.md` | ⚠ DURING | Unclaimed. The new pre-fork `TargetAccount` resolution reuses `resolve_pty_os_user`'s lookup; its tests enter that function. Promote to ✅ only if `/green` confirms the call graph. |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-ensure-project-available-for-start-ensure-project-available-for-start.md` | ⚠ DURING | The added failure/timeout log line goes in the `match` arm that already exists; no new branch. |
| `tddy-spawn` has no `docs/code-issues/` | ℹ | Not analyzed — not the same as clean. |

## Responsibility

Children started as another OS user either become that user or fail fast, and their pre-exec progress
is visible in the log. Owns:

- An async-signal-safe privilege drop shared by every daemon spawn: target account resolved **before**
  `fork` (uid, gid, supplementary groups via `getgrouplist`, home, name as `CString`); `setgroups` →
  `setgid` → `setuid` in the child; nothing else.
- A **pre-exec stage pipe**: a `CLOEXEC` pipe the child writes one byte per step to (`write(2)` only);
  the parent reads it, logs each step with the child's pid, and logs `pid N still at stage S after Ns`
  every heartbeat while a step is outstanding. Exec closes the pipe; EOF after `Exec` is success.
- The **fail-fast** check: a process whose effective uid is not 0 asked to start a child as a different
  uid refuses before forking.
- Applying all three to `clone_as_user_with_env`, `spawn_as_user` (`tddy-spawn/src/spawner.rs`) and
  `as_user_command` (`tddy-daemon-kernel/src/spawn_as_user.rs`); removing their `set_var` and
  `initgroups` calls.
- One `log::warn!` on the session-start clone's failure and deadline arms in
  `svc_ensure_project_available_for_start.rs`.

## Boundaries

- Does not change which backend runs a spawn (node 3), the worker protocol (node 2), or remove any
  deadline (nodes 4, 5).
- Does not stream clone output or pass `--progress` (node 5).
- Does not touch `tddy-supervisor`'s `spawn_broker` (already safe); reuses its approach, not its code
  path, because the kernel cannot depend on `tddy-supervisor`.
- Does not touch the sandbox `pre_exec` closures (`tddy-sandbox-darwin`, `tddy-sandbox-cgroups`).

## Dependencies

None — root node.

## Draft PR contract

```rust
// tddy-daemon-kernel/src/privilege_drop.rs
pub struct TargetAccount { pub uid: u32, pub gid: u32, pub groups: Vec<libc::gid_t>, pub home: String, pub name: std::ffi::CString }
pub fn resolve_target_account(os_user: &str) -> Result<TargetAccount, String>;   // pre-fork; getgrouplist
pub enum PrivilegeChange { None, Drop(TargetAccount) }
/// Refuses (before any fork) when euid != 0 and the target uid differs from euid.
pub fn plan_privilege_change(target: TargetAccount) -> Result<PrivilegeChange, CannotBecomeUser>;
pub struct CannotBecomeUser { pub running_as: String, pub requested: String }   // Display names both users + the supervisor hint

// tddy-daemon-kernel/src/pre_exec_stages.rs (new)
#[repr(u8)] pub enum PreExecStage { Forked = 1, Groups = 2, Gid = 3, Uid = 4, Exec = 5 }
pub struct StagedCommand { /* wraps std::process::Command */ }
impl StagedCommand {
    pub fn new(cmd: std::process::Command, label: &str, change: PrivilegeChange) -> Self;
    pub fn spawn(self) -> std::io::Result<StagedChild>;          // installs the async-signal-safe pre_exec
}
pub struct StagedChild { pub child: std::process::Child, /* stage watcher handle */ }
pub const STAGE_HEARTBEAT: std::time::Duration;                 // 5 s
/// Reads stage bytes from `reader` until EOF, logging each and a heartbeat while one is outstanding.
/// `StagedCommand` runs it on a thread; tests drive it with their own pipe and heartbeat.
pub fn watch_stages(reader: std::fs::File, pid: u32, label: &str, heartbeat: std::time::Duration) -> StageOutcome;
pub enum StageOutcome { Execed, StoppedAt(PreExecStage), NothingReported }
```

Failing tests (unit, in-crate, and integration under `tests/`) pin: fail-fast message, stage order for
a same-user child, the heartbeat for a child held at `Forked`, and that the three spawn paths go through
`StagedCommand`.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — every test spawns as the current user or asserts the refusal, no
other node's behaviour.
**Concurrent with:** #worker-mux (2/8), #supervisor-handoff (3/8), #split-stream (4/8)
**Blocks:** #clone-progress (5/8) — its streamed clone is built on `StagedCommand`

    n1 → n5      n2 → n5      n5 → n6, n7, n8      n3 → n6      n4 → n8      n6 → n8

## Affected Packages

- **tddy-daemon-kernel**: [README.md](../../../packages/tddy-daemon-kernel/README.md), [daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md) — `privilege_drop`, new `pre_exec_stages`, `spawn_as_user`
- **tddy-spawn**: [README.md](../../../packages/tddy-spawn/README.md) — `spawner.rs` clone and session spawns use `StagedCommand`
- **tddy-agent-launch**: [README.md](../../../packages/tddy-agent-launch/README.md) — one log line on the clone failure/deadline arms

## Related Feature Documentation

- PRD: [PRD-2026-10-24-start-progress-spawn-diag.md](../../ft/daemon/1-WIP/PRD-2026-10-24-start-progress-spawn-diag.md)
- Umbrella: [PRD-2026-10-24-start-progress.md](../../ft/daemon/1-WIP/PRD-2026-10-24-start-progress.md)

## Summary

Every daemon spawn as another OS user goes through one async-signal-safe privilege drop that refuses
up front when it cannot succeed and reports its pre-exec steps to the log.

## Background

2026-10-24: a clone child forked by an unprivileged worker hung forever before `exec` (pid 257936 on
`mantas-udoo-ros`, still uid `tddy`). `pre_exec` called `std::env::set_var` — `std` holds the env read
lock across `fork`, so the write lock never comes — and `initgroups(3)`, which enters NSS after
`fork`. Nothing logged where it was; the operator saw only a 30 s timeout. The same closure exists in
three places, one of them in the multi-threaded daemon.

## Scope

- [ ] Pre-fork `TargetAccount` resolution with supplementary groups
- [ ] Fail-fast `plan_privilege_change` with a message naming both users
- [ ] `StagedCommand` / `StagedChild`: async-signal-safe pre_exec, stage pipe, stage logs, heartbeat
- [ ] `clone_as_user_with_env`, `spawn_as_user`, `as_user_command` moved onto it; `set_var` / `initgroups` gone
- [ ] Clone failure / deadline logged on the cloning host
- [ ] Docs: `daemon-kernel.md` privilege drop section, `tddy-spawn` README

## Technical Changes

### State A
Three `pre_exec` closures (`spawner.rs:707`, `spawner.rs:1269`, `spawn_as_user.rs:214`) each run
`set_var("HOME")`, `setgid`, `initgroups`, `setuid` in the child. No privilege check before fork; a
non-root process gets EPERM at best and a deadlock at worst. The parent learns nothing until `exec`
or an errno. `same_user` compares **real** ids. `ensure_project_available_for_start` returns
`deadline_exceeded` with no log line.

### State B
One `StagedCommand` path. The parent resolves the account and groups, refuses if it lacks the
privilege, forks; the child writes `Forked`, `Groups`, `Gid`, `Uid` bytes around three bare syscalls
and execs (`Exec` byte written immediately before `execve`; the `CLOEXEC` pipe closes on success).
A watcher thread logs `spawn <label> pid=<n> reached <stage>` per byte and
`spawn <label> pid=<n> still at <stage> after <secs>s` every `STAGE_HEARTBEAT`. The same-user path
writes `Forked` and `Exec` only. Identity comparisons use **effective** ids.

### Delta
- `tddy-daemon-kernel`: `privilege_drop.rs` gains `TargetAccount`, `resolve_target_account`,
  `PrivilegeChange`, `plan_privilege_change`, `CannotBecomeUser`; new `pre_exec_stages.rs`;
  `spawn_as_user.rs::as_user_command` uses them.
- `tddy-spawn`: `clone_as_user_with_env` and `spawn_as_user` use them; their passwd lookups and closures
  are deleted.
- `tddy-agent-launch`: `svc_ensure_project_available_for_start.rs` logs the failure and deadline arms.

## Testing Plan

Unit tests for the pure parts (privilege plan, stage decoding). Real-process integration tests in
`tddy-daemon-kernel/tests/` spawning as the **current** user (no root needed) pin the stage order. The
heartbeat is tested on the stage watcher itself, fed by a test-owned pipe that stops after `Forked`
(see Decisions — freezing a real child mid-`pre_exec` would need a test-only production hook).
Fail-fast is tested by asking for `root` (uid 0) from
a non-root test process — refused before fork on any developer machine and CI runner. A source-scan
test asserts no `pre_exec` closure in the three files calls `set_var`/`initgroups`.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_non_root_process_asked_for_another_user_refuses_before_forking` | `packages/tddy-daemon-kernel/tests/privilege_change_acceptance.rs` (new) | `CannotBecomeUser` names both users and the supervisor; no child spawned |
| `a_same_user_child_reports_forked_then_exec_and_runs` | `packages/tddy-daemon-kernel/tests/pre_exec_stages_acceptance.rs` (new) | stage order logged with the pid; child output intact |
| `a_child_held_at_a_stage_is_logged_as_still_there` | same | heartbeat line `still at <stage> after <n>s` |
| `a_clone_as_another_user_from_a_non_root_worker_fails_fast_naming_both_users` | `packages/tddy-spawn/tests/clone_privilege_acceptance.rs` (new) | `clone_as_user_with_env` refuses; no destination created |
| `no_pre_exec_closure_touches_the_environment_or_nss` | `packages/tddy-spawn/tests/pre_exec_safety.rs` (new) | `spawner.rs`, `spawn_as_user.rs` contain no `set_var`/`initgroups` in `pre_exec` |

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Kernel owns the drop.** `tddy-spawn` depends on `tddy-daemon-kernel`, not the reverse, and the
  kernel's own `as_user_command` needs it; `privilege_drop.rs` is already "becoming another OS user".
- **Effective ids** for the "same user" decision, matching `spawn_broker`; a setuid-root edge case is
  otherwise misjudged.
- **The held-child heartbeat is tested on the watcher, not by freezing a real child.** Holding a
  child mid-`pre_exec` needs a test-only hook in production code, which AGENTS.md forbids.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-24-start-progress-spawn-diag-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the developer for the whole of pass 2, 2026-10-24)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages (`./test -p tddy-daemon-kernel -p tddy-spawn -p tddy-agent-launch`); full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped clippy, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
