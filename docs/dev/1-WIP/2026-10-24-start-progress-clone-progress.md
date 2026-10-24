# Changeset: A start that clones reports git's progress

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — proto contract change, several crates (service, spawn, projects, session-files, agent-launch, daemon-rpc, web generated code), a spawned-process streaming seam, and a filesystem ownership change (temp-dir clone + rename)
**Stack**: #start-progress 5/8. Base: `feature/start-progress/split-stream`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-24-start-progress-clone-progress-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-10-17-a-timed-out-clone-leaves-a-partial-destination-that-later-starts-skip.md](../todo/2026-10-17-a-timed-out-clone-leaves-a-partial-destination-that-later-starts-skip.md) | ✅ RESOLVED HERE | Clones go to a temporary sibling and are renamed on success; failure or stop removes the temporary directory; the clone child is killable. |
| [2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md](../todo/2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md) | ⚠ DURING | The parser and the streamed clone go in new modules (`git_progress.rs`, `clone_run.rs`); `spawner.rs` loses `clone_as_user_with_env`'s body. |
| [2026-10-17-clone-passphrase-prompt-verification-gaps.md](../todo/2026-10-17-clone-passphrase-prompt-verification-gaps.md) | — | The askpass environment is carried unchanged. |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-ensure-project-available-for-start-ensure-project-available-for-start.md` | ⚠ DURING | The deadline `match` goes away (net shorter); the progress sink is passed into `spawn_project_clone`, not threaded through new branches. |
| `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md` | ⚠ DURING | Removing the two `spawn_blocking_with_timeout` / `await_supervised_with_timeout` wrappers shrinks the file. |
| `packages/tddy-daemon-rpc/docs/code-issues/complexity-project-coordinate-handlers-add-project-to-host-at-project-coordinate.md` | ⚠ DURING | Same; shorter. |

## Responsibility

Clone progress and an unbounded, clean clone, for the forked-worker and in-process backends. Owns:

- `session.proto`: `StartPhase.Step.STEP_CLONE = 4`; `StartSessionEvent.clone_progress = 5` with
  `CloneProgress { stage, percent, objects_done, objects_total, bytes_received, bytes_per_second, line }`;
  regenerated Rust and `packages/tddy-web/src/gen/session_pb.ts`.
- `tddy-spawn`: `git_progress::parse` (git's `\r`-separated stderr progress → `GitProgress`);
  `clone_run`: `git clone --progress` into `<dest>.partial-<uuid>`, stderr streamed and parsed, events
  throttled (≤ 4/s plus every stage change), rename on success, remove on failure, a `CloneHandle` that
  kills the clone's process group; `clone_as_user_with_env` becomes a thin call into it.
- The worker carries progress as `WorkerEvent::CloneProgress` over node 2's events.
- `tddy-projects`: the cloner type gains a progress callback.
- `tddy-session-files`: `AttachmentProgressSink::clone_progress(..)`.
- `tddy-agent-launch`: `provision_project_for_start` brackets a real clone with `STEP_CLONE` and feeds
  progress into the sink; the clone deadline (`tokio::time::timeout(spawn_worker_request_timeout, …)`)
  is removed.
- `tddy-daemon-rpc`: CreateProject / AddProjectToHost clones lose their deadline wrappers (still unary,
  no progress).
- A TODO for streaming CreateProject / AddProjectToHost.

## Boundaries

- Does not stream the **supervised** clone (node 6): on a supervised host a start emits `STEP_CLONE`
  begin/end with no progress events until node 6.
- Does not render anything in the web UI (node 7); only regenerates `session_pb.ts`.
- Does not cancel a start (node 8) — it provides `CloneHandle::stop`, which node 8 calls.
- Does not change which repositories clone, where, or the askpass environment.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/start-progress/spawn-diag` (1/8) | `StagedCommand` / `StagedChild` (privilege drop, stage logs) in `tddy-daemon-kernel` | `clone_run` spawns git through it, with stderr piped | change the drop, the stage pipe or fail-fast |
| `feature/start-progress/worker-mux` (2/8) | request ids, `WorkerEvent`, `SpawnClient::clone_repo_observed` | adds `WorkerEvent::CloneProgress` and sends it from the worker's clone | change the wire envelope, the client's routing or the worker's threading |
| `feature/start-progress/split-stream` (4/8) | relayed start events across the split hop | nothing directly; clone events relay as any other event | touch forwarding |
| `feature/start-progress/supervisor-handoff` (3/8) | injected `SpawnBackendChoice` | `CloneBackend` keeps dispatching on it | touch the choice |

## Draft PR contract

```proto
// session.proto
enum Step { …; STEP_CLONE = 4; }                       // in StartPhase
message CloneProgress {
  string stage = 1;               // git's own words: "Counting objects", "Receiving objects", "Resolving deltas"
  uint32 percent = 2;
  uint64 objects_done = 3; uint64 objects_total = 4;
  uint64 bytes_received = 5;      // 0 when git did not report bytes for this stage
  uint64 bytes_per_second = 6;    // 0 when git did not report throughput
  string line = 7;                // the raw progress line, for display when parsing found no stage
}
// StartSessionEvent: CloneProgress clone_progress = 5;
```

```rust
// tddy-spawn/src/git_progress.rs (new)
pub struct GitProgress { pub stage: String, pub percent: u32, pub objects_done: u64, pub objects_total: u64,
                         pub bytes_received: u64, pub bytes_per_second: u64, pub line: String }
pub fn parse(line: &str) -> Option<GitProgress>;
pub struct ProgressThrottle { /* ≤ 4/s, always passes a stage change and 100% */ }

// tddy-spawn/src/clone_run.rs (new)
pub struct CloneHandle { /* pid, process group */ }
impl CloneHandle { pub fn stop(&self) -> std::io::Result<()>; }
pub fn clone_with_progress(os_user: &str, git_url: &str, destination: &Path, env: &[(&str, &str)],
    on_started: impl FnOnce(CloneHandle), on_progress: impl FnMut(GitProgress)) -> anyhow::Result<()>;

// tddy-spawn/src/spawn_worker/wire.rs — WorkerEvent::CloneProgress(GitProgress)
// tddy-projects/src/project_provision.rs — C: Fn(&str, &Path, &mut dyn FnMut(GitProgress)) -> Result<(), String>
// tddy-session-files — AttachmentProgressSink::clone_progress(&self, CloneProgress)
```

## Green wave

**Wave:** 2 of 4
**Greenable independently:** no — `clone_run` spawns through node 1's `StagedCommand` and the worker
streams over node 2's events; both must be green. Nothing from nodes 3 or 4 is exercised.
**Concurrent with:** none (alone in wave 2)
**Blocks:** supervised-clone-progress (6/8), progress-ui (7/8), start-cancel (8/8)

    n1 → n5      n2 → n5      n5 → n6, n7, n8      n3 → n6      n4 → n8      n6 → n8

## Affected Packages

- **tddy-service**: `proto/session.proto` (+ generated code)
- **tddy-spawn**: [README.md](../../../packages/tddy-spawn/README.md), [clone-ssh-environment.md](../../../packages/tddy-spawn/docs/clone-ssh-environment.md) — streamed clone
- **tddy-projects**: cloner signature
- **tddy-session-files**: sink method
- **tddy-agent-launch**: [README.md](../../../packages/tddy-agent-launch/README.md) — clone step, no deadline
- **tddy-daemon-rpc**: CreateProject / AddProjectToHost deadline removal
- **tddy-web**: `src/gen/session_pb.ts` regenerated only

## Related Feature Documentation

- PRD: [PRD-2026-10-24-start-progress-clone-progress.md](../../ft/daemon/1-WIP/PRD-2026-10-24-start-progress-clone-progress.md)
- [project-concept.md](../../ft/daemon/project-concept.md), [clone-ssh-passphrase.md](../../ft/daemon/clone-ssh-passphrase.md)

## Summary

A start that clones reports a clone step with git's parsed progress; clones are unbounded, killable,
and leave no partial destination.

## Background

No clone passes `--progress`; output is buffered by `Command::output()` and surfaced only on failure;
the session-start clone and the project-coordinate clones sit under `spawn_worker_request_timeout`
(abandoning, not killing, the child); a killed or failed clone can leave `<repos>/<name>` that the next
start's `test -e` skips.

## Scope

- [ ] Proto: `STEP_CLONE`, `CloneProgress`, `clone_progress` event; generated Rust + TS
- [ ] `git_progress::parse` + `ProgressThrottle`
- [ ] `clone_with_progress`: `--progress`, temp dir + rename, remove on failure, `CloneHandle::stop`
- [ ] Worker relays `WorkerEvent::CloneProgress`
- [ ] Cloner signature with progress; sink `clone_progress`
- [ ] Session start: `STEP_CLONE` + progress; deadline removed
- [ ] CreateProject / AddProjectToHost: deadline removed
- [ ] TODO: stream CreateProject / AddProjectToHost
- [ ] Docs: project-concept (clone step), tddy-spawn clone docs

## Technical Changes

### State A
`sh -c 'test -e "$1" || exec git clone "$2" "$1"'` with `.output()`; supervisor path `git clone`
without `--progress`; `StartPhase` has WORKTREE / SEMANTIC_INDEX / AGENT; `provision_project_for_start`
under `with_keepalive` and a `tokio::time::timeout`; the cloner is `Fn(&str,&Path)->Result<(),String>`.

### State B
`git clone --progress <url> <dest>.partial-<uuid>` spawned through `StagedCommand` with stderr piped
in its own process group; a reader splits on `\r`/`\n`, parses, throttles and calls back; exit 0 →
`rename(partial, dest)`; otherwise remove `partial` and fail with git's last error lines. The session
start sends `STEP_CLONE` BEGIN, `clone_progress` events, END, then the worktree step. No deadline.

## Testing Plan

Parser unit tests on real git progress lines (captured from git 2.4x, including byte/throughput and
"done." variants). Integration tests run a **real** `git clone --progress` of a local bare repository
large enough to report progress (generated in a tempdir), and a fake `git` on `PATH` (carried in the
clone env) to script slow/failing/hanging clones. Session-level test in `start_phase_acceptance.rs`
with a project whose main repo is absent so the start clones from a local origin.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `git_progress_lines_parse_into_stage_percent_counts_bytes_and_throughput` | `packages/tddy-spawn/tests/git_progress.rs` (new) | parser on real lines |
| `a_throttle_passes_stage_changes_and_completion_but_not_every_tick` | same | throttle |
| `a_clone_reports_progress_and_lands_at_its_destination` | `packages/tddy-spawn/tests/clone_progress_acceptance.rs` (new) | real git, temp + rename |
| `a_failed_clone_leaves_no_destination_and_carries_gits_message` | same | cleanup + error |
| `a_stopped_clone_ends_its_git_process_and_leaves_no_destination` | same | `CloneHandle::stop` |
| `the_worker_relays_clone_progress_before_the_clone_result` | `packages/tddy-spawn/tests/worker_concurrency_acceptance.rs` | worker events |
| `a_start_that_clones_reports_the_clone_step_and_its_progress_before_the_worktree` | `packages/tddy-session-lifecycle/tests/start_phase_acceptance.rs` | session events |
| `a_start_whose_project_is_present_reports_no_clone_step` | same | no spurious step |
| `a_start_clone_outlives_the_spawn_worker_request_timeout` | same | no deadline |
| `create_project_clones_past_the_spawn_worker_request_timeout` | `packages/tddy-daemon-rpc/tests/create_project_acceptance.rs` | no deadline |

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Complexity 5 accepted.** The proto event, the parser and the emitter are one capability; splitting
  them would ship a field nothing writes. The supervised backend is already split off (node 6).
- **Temp sibling + rename** (same filesystem, atomic) over remove-on-failure alone: a crash mid-clone
  leaves only a `.partial-*` directory the next start ignores.
- **Throttle at the source** (worker / clone runner), so neither the worker pipe nor the LiveKit relay
  carries git's per-tick redraws.

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

- [x] Record initial discovery (`2026-10-24-start-progress-clone-progress-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests (`./test -p tddy-spawn -p tddy-projects -p tddy-session-files -p tddy-agent-launch -p tddy-session-lifecycle -p tddy-daemon-rpc`); full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped clippy, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
