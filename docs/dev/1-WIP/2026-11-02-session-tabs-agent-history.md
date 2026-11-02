# Changeset: The agent terminal's output outlives its process

**Created:** 2026-11-02
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — four crates (`tddy-task`, `tddy-cli-sessions`, `tddy-terminal-rpc`, `tddy-session-lifecycle`, plus the sandbox registry's exit path); an on-disk format; a crash-safety interval; offsets must page exactly like the live capture
**Stack:** `#session-tabs 2/7` — branch `feature/session-tabs/agent-history`, base `feature/session-tabs/dormant-tabs` · **PR:** [#805](https://github.com/uppin/tddy-coder/pull/805)
**PRD:** [PRD-2026-11-02-agent-terminal-history-persists.md](../../ft/daemon/1-WIP/PRD-2026-11-02-agent-terminal-history-persists.md)

## Initial Discovery

[2026-11-02-session-tabs-agent-history-initial-discovery.md](./2026-11-02-session-tabs-agent-history-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| `packages/tddy-task/docs/code-issues/oversized-file-terminal-capture.md` | ⚠ During — the recording is a new module (`capture_recording.rs`) built on the capture's public accessors; `terminal_capture.rs` gains no lines |
| `packages/tddy-task/docs/code-issues/complexity-terminal-capture-feed.md` | ⚠ During — the feed path is not touched |
| `packages/tddy-terminal-rpc/docs/code-issues/oversized-file-bridge.md` | ⚠ During — recorded history is served from a new module, not `bridge.rs` |

## Affected Packages

- [ ] [`tddy-task`](../../../packages/tddy-task/README.md): `CaptureRecording` — a serializable snapshot of a capture's retained window with its offsets, and paging over it
- [ ] [`tddy-cli-sessions`](../../../packages/tddy-cli-sessions/README.md): record a session's `main` capture on exit and periodically
- [ ] [`tddy-daemon-sandbox`](../../../packages/tddy-daemon-sandbox/README.md): the same for a jailed `main`
- [ ] [`tddy-terminal-rpc`](../../../packages/tddy-terminal-rpc/README.md): a `RecordedTerminalHistory` port that `GetTerminalHistory` reads when no terminal is running
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): the daemon's implementation reading `<session_dir>/terminals/main.capture`

## Responsibility

- `tddy_task::capture_recording::CaptureRecording { start_offset: u64, end_offset: u64, data: Vec<u8> }`:
  - `CaptureRecording::of(&TerminalCapture) -> Self`;
  - `replay_from(from_offset, until_offset, max_bytes) -> CaptureChunk`, with the same semantics as
    `TerminalCapture::replay_from`;
  - `write_atomically(&Path) -> io::Result<()>` (temp file + rename) and `read(&Path) -> io::Result<Self>`;
  - a versioned header that refuses a wrong magic or version.
- `CliSessionManager::record_main_capture(session_id, path: PathBuf)` — registers where a session's `main`
  capture is recorded. It is written every `RECORDING_INTERVAL` (30 s) while `main` runs and once more on
  its exit, before deregistration. `CliSessionManager::with_recording_interval(Duration)` overrides the
  interval (`capture_recorder.rs`).
- The launch and resume paths (`tddy-agent-launch`) call it with
  `tddy_session_lifecycle::session_dir_recorded_history::main_capture_recording_path(tddy_data_dir, id)`
  (`<tddy_data_dir>/sessions/<id>/terminals/main.capture`).
- The jailed equivalent, `SandboxSessionManager::record_main_capture(session_id, path)`.
- `tddy_terminal_rpc::service::RecordedTerminalHistory` trait:
  `async fn recorded(&self, os_user: &str, session_id: &str, terminal_id: &str) -> Option<CaptureRecording>`.
  - Added to `TerminalSessionPorts` as `recorded_history`; `NoRecordedHistory` is the host that records
    nothing.
  - `get_terminal_history` consults it only when the store has no running terminal. It is served from
    a new module, `service/recorded_history.rs`.
- The daemon's `SessionDirRecordedHistory` (`tddy-session-lifecycle`), which answers `main` from the
  file and `None` for any other terminal id.
- The coder participant (and its parity suite) passes `NoRecordedHistory`.

## Boundaries

- No web change (n6 shows it).
- No recording of started tabs.
- `StreamTerminalIO`, `SendTerminalInput` and resize are unchanged and still `not_found` for a stopped
  terminal.
- No change to the capture's size bound.
- Does not touch n1's worktree resolution or `TerminalLaunch`.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` dormant-tabs | `recorded_worktree` and the roster's `tddy_data_dir` field in `svc_terminal_ports.rs` | nothing at the code level — n1 sits below only because the line is flattened. This node adds a sibling field (`recorded_history`) to the same `TerminalSessionPorts` literal, which is a textual neighbour, not a dependency | change the roster, `recorded_worktree`, or the stopped-session start path |

## Draft PR contract

The first push publishes:

- `CaptureRecording` with its methods as `// TODO(agent-history): implement`;
- `CliSessionManager::record_main_capture`, a no-op body;
- the `RecordedTerminalHistory` trait, `NoRecordedHistory` and the `recorded_history` port field
  (`get_terminal_history` does not consult it yet), with the daemon's `SessionDirRecordedHistory`
  answering `None`;
- `SandboxSessionManager::record_main_capture`, a no-op body;
- the failing tests below.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes — its tests drive this node's own recorder and port against a real `CliSessionManager` running a short-lived process
**Concurrent with:** n1 `dormant-tabs`, n3 `tmux-attach`, n4 `osc52-clipboard`, n5 `tab-plus`
**Blocks:** n6 `dormant-agent-tab` in production: without a recording, its greyed view is empty. n6's tests use a double, so n6 can be greened alongside

Real dependency edges, as opposed to the branch line:

    n1 → n6      n2 → n6      n3 → n7

## State A

- `GetTerminalHistory` → `store.get_terminal` → `not_found("terminal not found or not running")` for any
  terminal not in a live registry. The capture is dropped with the PTY on exit.
- `TerminalCapture` exposes `start_offset()`, `end_offset()`, `buffered_bytes()` and `replay_from`; it
  has no serialization.

## State B

- `<session_dir>/terminals/main.capture` holds the latest retained window: rewritten every 30 s while
  `main` runs, and at exit.
- `GetTerminalHistory(main)` on a stopped session pages over it with identical offsets.
- A daemon restart reads it back.

## Delta

### New
- `packages/tddy-task/src/capture_recording.rs`
- `packages/tddy-cli-sessions/src/cli_session_manager/capture_recorder.rs`
- `packages/tddy-terminal-rpc/src/service/recorded_history.rs`
- `packages/tddy-session-lifecycle/src/session_dir_recorded_history.rs`

### Modified
- `tddy-cli-sessions` `pty_spawn.rs` (exit-time write before deregistration); the launch and resume call sites of `main` (`tddy-agent-launch`) register the path.
- `tddy-daemon-sandbox` `sandbox_session.rs` (jailed `main` recording).
- `tddy-terminal-rpc` `service/ports.rs`, `service.rs`.
- `tddy-coder` `session_participant/terminal_session_service.rs` (a `None` port).
- Docs, at wrap: `docs/ft/web/terminal-replay-lazy-scroll.md`, `docs/ft/daemon/claude-cli-session.md`, `packages/tddy-task/docs/`.

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`**: `packages/tddy-session-lifecycle/tests/agent_terminal_history_recording_acceptance.rs`

- [ ] `main_history_is_served_after_the_agent_process_exits`
- [ ] `main_history_is_served_by_a_fresh_daemon_over_the_same_session_dir`
- [ ] `paged_reads_over_a_recording_match_the_live_capture_byte_for_byte`
- [ ] `a_running_main_serves_its_live_capture_not_the_recording`
- [ ] `streaming_a_stopped_main_is_still_not_found`
- [ ] `a_session_that_never_recorded_answers_not_found`

These register the recording the way the launcher does (`record_main_capture` at
`main_capture_recording_path`). **That the launcher itself registers it is not pinned here.** `/green`
adds that test where the launch and resume paths are exercised with a stub agent (an
`svc_resume_claude_cli_session` / start-session case asserting the file appears).

### Unit / integration tests

- [ ] `packages/tddy-task/tests/capture_recording_unit.rs`:
  - `a_recording_holds_the_window_and_offsets_its_capture_retains`;
  - `a_recording_written_and_read_back_is_the_same_recording`;
  - `paging_a_recording_answers_the_chunks_its_live_capture_answers`;
  - `a_file_that_is_not_a_recording_is_refused_as_invalid_data`;
  - `a_failed_write_leaves_no_file_behind`.
  - `tempfile` becomes a dev-dependency of `tddy-task` (already in the workspace lock).
- [ ] `packages/tddy-cli-sessions/tests/capture_recorder.rs` (a real PTY, a 50 ms interval):
  - `a_registered_main_is_recorded_with_its_last_output_before_it_leaves_the_index`;
  - `a_running_main_is_recorded_on_the_interval`;
  - `an_unregistered_session_is_never_recorded` (a regression guard: it passes on the contract).
- [ ] `packages/tddy-terminal-rpc/tests/recorded_history_acceptance.rs` (the shared `support` host gains
  `serving_a_recorded_main_terminal` and `serving_main_terminal_with_a_stale_recording`):
  - `a_stopped_main_terminal_answers_history_from_its_recording`;
  - `a_running_main_terminal_never_reads_its_recording` (a regression guard).

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Format and paging | Unit over `CaptureRecording` | Pure data |
| Recorder timing | Integration over a real PTY with a short configured interval | A paused tokio clock does not pause a child process |
| Served behaviour | Daemon acceptance through `terminal_session_service()` | The coordinate the web calls |

## Decisions & Trade-offs

- **The latest window, not a log.** It has the same bound as the live capture, so disk use is fixed
  per session and paging semantics are identical.
- **Periodic plus at-exit writes.** At-exit alone loses everything on a daemon crash; continuous
  append would need a new log format.
- **A port consulted only when nothing runs** is the designed source for a stopped terminal, not a
  fallback over a live one. A running `main` never reads the file.

## Baseline before green

Recorded at the contract commit (2026-11-02), scoped to `tddy-task`, `tddy-cli-sessions`, `tddy-terminal-rpc`, `tddy-session-lifecycle`, `tddy-daemon-sandbox`, `tddy-coder`:

- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean (`tddy-daemon-sandbox` at `--lib`: see below).
- **This node's red tests:**
  - `capture_recording_unit.rs`: 5 failing;
  - `capture_recorder.rs`: 2 failing (+ `an_unregistered_session_is_never_recorded` passing as a guard);
  - `recorded_history_acceptance.rs`: 1 failing (+ the running-terminal guard passing);
  - `agent_terminal_history_recording_acceptance.rs`: 4 failing (+ the stopped-stream and never-recorded refusals passing as guards).
- Unchanged and green: `terminal_session_acceptance`, `agent_terminal_tabs_acceptance`, `tddy-coder` `two_server_parity_acceptance`, every other `tddy-task` / `tddy-cli-sessions` / `tddy-terminal-rpc` suite.
- **Not this node:** `tddy-daemon-sandbox` `tests/sandbox_stdio_seatbelt_acceptance.rs` (macOS-gated) does not compile on master (`cannot find type SandboxHandle`), so that crate's clippy runs at `--lib`. The `tddy-session-lifecycle` environmental failures recorded in #804's baseline (sibling binaries not built under a bare `cargo test -p`) apply here too.

## TODO

- [x] Record initial discovery (`2026-11-02-session-tabs-agent-history-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — waived by the developer 2026-11-02 (reviewed with the stack at the end of pass 2)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
