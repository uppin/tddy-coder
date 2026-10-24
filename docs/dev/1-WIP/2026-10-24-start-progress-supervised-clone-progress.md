# Changeset: Supervised clones report progress and their own errors

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — proto contract change (`supervisor.proto`), two crates (`tddy-supervisor`, `tddy-spawn`), a privileged spawned-process seam with a new streaming RPC
**Stack**: #start-progress 6/8. Base: `feature/start-progress/clone-progress`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-24-start-progress-supervised-clone-progress-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-10-17-clone-passphrase-prompt-verification-gaps.md](../todo/2026-10-17-clone-passphrase-prompt-verification-gaps.md) | ⚠ DURING | Its third bullet ("a supervisor refusal is not named") is adjacent: this node returns git's stderr tail, not the refused key. Not resolved; the refusal text is unchanged. |
| `packages/tddy-supervisor/docs/code-issues/complexity-server-serve.md` | ⚠ DURING | `serve` is not modified; the new RPC is a handler method beside `spawn_session`. |
| [2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md](../todo/2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md) | ⚠ DURING | `supervisor_spawn.rs` (386) gains the streamed client call and loses `wait_for_exit` polling for clones; net not larger. |

## Responsibility

The supervised clone backend at parity with node 5. Owns:

- `supervisor.proto`: `rpc CloneRepository(CloneRepositoryRequest) returns (stream CloneEvent)`;
  `CloneEvent { oneof { Started{pid}, ProgressLine{line}, Exited{code, signal, stderr_tail} } }`.
- `tddy-supervisor`: the handler resolves the spawn under the existing policy (`resolve_spawn`: user,
  `allowed_env_keys`, scope), runs `git clone --progress <url> <dest>.partial-<uuid>` as the target user
  with **stderr piped** (stdout null), streams progress lines, renames on success / removes on failure,
  ends with `Exited`. `StopSession(pid)` stops it.
- `tddy-spawn::supervisor_spawn`: `clone_repo_via_supervisor_with_progress` consuming the stream into
  node 5's `GitProgress` callbacks and `CloneHandle`; the polling clone path is removed.

## Boundaries

- Does not stream any other supervisor RPC, or change `SpawnSession`'s `/dev/null` output policy for
  sessions.
- Does not change authorization, user or env policy.
- Does not change the progress event, parser or throttle (node 5's).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/start-progress/clone-progress` (5/8) | `git_progress::{parse, GitProgress, ProgressThrottle}`, `CloneHandle`, `CloneBackend` progress callback, `STEP_CLONE` emission | parses supervisor progress lines with node 5's parser; returns a `CloneHandle` | change the proto event, parser, throttle or session emission |
| `feature/start-progress/supervisor-handoff` (3/8) | `SpawnBackendChoice::Supervisor { socket_path }` from the environment | the supervised clone path is selected by it | change the choice |

## Draft PR contract

```proto
// tddy-supervisor/proto/supervisor.proto
rpc CloneRepository(CloneRepositoryRequest) returns (stream CloneEvent);
message CloneRepositoryRequest { string os_user = 1; string git_url = 2; string destination = 3; map<string,string> env = 4; }
message CloneEvent { oneof event { CloneStarted started = 1; CloneProgressLine progress = 2; CloneExited exited = 3; } }
message CloneStarted { uint32 pid = 1; }
message CloneProgressLine { string line = 1; }
message CloneExited { optional int32 code = 1; optional int32 signal = 2; string stderr_tail = 3; }
```

```rust
// tddy-supervisor/src/client.rs
impl SupervisorClient { pub async fn clone_repository(&self, req: CloneRequest) -> Result<impl Stream<Item = Result<CloneEvent, SupervisorError>>, SupervisorError>; }
// tddy-spawn/src/supervisor_spawn.rs
pub async fn clone_repo_via_supervisor_with_progress(socket: &Path, os_user: &str, git_url: &str, dest: &Path,
    env: BTreeMap<String,String>, on_started: impl FnOnce(CloneHandle), on_progress: impl FnMut(GitProgress)) -> anyhow::Result<()>;
```

## Green wave

**Wave:** 3 of 4
**Greenable independently:** no — needs node 5's parser and `CloneHandle` green; the supervisor-side
tests alone (`tddy-supervisor`) need nothing.
**Concurrent with:** progress-ui (7/8)
**Blocks:** start-cancel (8/8) — cancel stops a supervised clone through this RPC's pid

    n1 → n5      n2 → n5      n5 → n6, n7, n8      n3 → n6      n4 → n8      n6 → n8

## Affected Packages

- **tddy-supervisor**: [README.md](../../../packages/tddy-supervisor/README.md), `proto/supervisor.proto`
- **tddy-spawn**: [README.md](../../../packages/tddy-spawn/README.md) — supervised clone

## Related Feature Documentation

- PRD: [PRD-2026-10-24-start-progress-supervised-clone-progress.md](../../ft/daemon/1-WIP/PRD-2026-10-24-start-progress-supervised-clone-progress.md)
- [tddy-supervisor.md](../../ft/supervisor/tddy-supervisor.md)

## Summary

The supervisor gains a streaming clone RPC; supervised session-start clones report progress, carry
git's error text, and are stoppable.

## Background

Supervised clones are `SpawnSession{git clone}` + 25 ms `SessionStatus` polling with output to
`/dev/null`: no progress, and a failure is only "exited with code N".

## Scope

- [ ] `CloneRepository` streaming RPC in the proto, server and client
- [ ] Supervisor runs git as the target user under spawn policy, stderr piped, temp + rename
- [ ] `clone_repo_via_supervisor_with_progress`; polling clone removed
- [ ] Docs: supervisor README / `tddy-supervisor.md` RPC list

## Technical Changes

### State A
All 11 supervisor RPCs unary; clone = spawn + poll; no stderr; `inherit_output: false`.

### State B
`CloneRepository` streams `Started{pid}`, progress lines and `Exited{code, signal, stderr_tail}`;
the daemon maps lines through node 5's parser into start events and fails with the stderr tail.

## Testing Plan

Supervisor integration tests (existing `tests/support` harness running the supervisor in-process as
the current user, as `supervisor_session_lifetime.rs` does) with a real local bare repository and a
fake git for failure; a daemon-side test through `clone_repo_via_supervisor_with_progress` against
that in-process supervisor.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_supervised_clone_streams_progress_then_exits_zero` | `packages/tddy-supervisor/tests/supervisor_clone_stream.rs` (new) | stream shape |
| `a_failed_supervised_clone_ends_with_gits_stderr_tail` | same | error text |
| `a_supervised_clone_with_an_unlisted_env_key_is_refused` | same | policy unchanged |
| `stopping_a_supervised_clone_ends_git_and_leaves_no_destination` | same | `StopSession(pid)` + cleanup |
| `the_daemon_reports_supervised_clone_progress_through_the_clone_callbacks` | `packages/tddy-spawn/tests/supervised_clone_progress.rs` (new) | daemon side |

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **A dedicated RPC**, not `inherit_output` on `SpawnSession`: sessions keep their no-output policy, and
  the clone's lifetime is one stream.
- **The supervisor owns temp + rename** for supervised clones, so the daemon (unprivileged) never needs
  write access to the user's repos directory.

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

- [x] Record initial discovery (`2026-10-24-start-progress-supervised-clone-progress-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests (`./test -p tddy-supervisor -p tddy-spawn`); full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped clippy, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
