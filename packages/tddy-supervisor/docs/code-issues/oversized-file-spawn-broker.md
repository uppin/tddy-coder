# oversized-file: spawn_broker.rs

**Location:** `packages/tddy-supervisor/src/spawn_broker.rs`
**Category:** oversized-file
**Detected:** 2026-10-24 — `/pr-wrap` step 3.5 file-length gate on #736 (`#start-progress` 3/8), and again on `#coagents` 2/8 (#721), production lines counted to the first `#[cfg(test)]`
**Metrics:** **968 production lines** · budget 500
**Thresholds breached:** length 968 > 500
**Restructure:** blocked — the remaining code is the pre-exec syscall layer, about 630 lines, most of it defined twice (one item per `#[cfg(target_os)]` branch)
**Status:** Open — partially fixed 2026-10-24 (1,595 → 968 by #721's split, measured after #721 was rebased onto #start-progress); unclaimed
**Verified:** the engine refusals below were reproduced by hand on 2026-10-24; the 968 is a machine-measured line count after the rebase

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-10-24 | 1,526 → 1,549 | grown by #736: ChildEnvironment / EnvironmentBase::Inherited carrying the supervisor socket, and the strip-then-insert of TDDY_SUPERVISOR_SOCKET |
| 2026-10-24 | 1,549 → 1,595 | grown by #739: `ChildOutput` replacing `inherit_output`, the stderr pipe for `CaptureStderr` — split deferred with the developer's consent, #start-progress 6/8 |
| 2026-10-24 | 1,526 → 1,594 → 944 | #721, measured on its pre-#start-progress base: grew it by 68 (`StdioMode`, `ChildStdio`, the piped spawn) and then split it with engine `move_item`s into `spawn_broker/fork_broker.rs` (329), `child_environment.rs` (185) and `accounts.rs` (180). The rest could not move: see below |
| 2026-10-25 | 968 | #721 rebased onto master's #start-progress: the split kept, #736/#739's additions carried into the moved files (`fork_broker.rs` 372, `child_environment.rs` 196); `SUPERVISOR_SOCKET_ENV` stays here as the public path |

## Why the engine stops here

`tddy-tools restructure` refuses to name an item that is defined once per `#[cfg(target_os)]` branch
(`plan is malformed: … names 2 items … qualify it with the trait`; a `#N` suffix is refused too), and
`anchors --at` inside that region answers "inside no item". That covers `GroupListId`, `GroupCount`,
`set_parent_death_signal`, `RESOLVE_NO_SYMLINKS` / `RESOLVE_BENEATH`, `open_directory_reference`,
`open_resolved`, the `enter_*_namespace` functions, `make_root_mount_private`, `bind_mount` and
`bring_loopback_up`.

## What would close it

Either the engine learns to address a `cfg`-duplicated item (by its `cfg` predicate), or the
platform pairs are restructured in place so each is one item with a `cfg` inside — then move the
namespace/mount/landlock syscalls into `spawn_broker/pre_exec_linux.rs` and the stubs beside them.
