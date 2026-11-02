# Changeset: The agent terminal's output outlives its process

**Created:** 2026-11-02
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — four crates (`tddy-task`, `tddy-cli-sessions`, `tddy-terminal-rpc`, `tddy-session-lifecycle`, plus the sandbox registry's exit path); an on-disk format; a crash-safety interval; offsets must page exactly like the live capture
**Stack:** `#session-tabs 2/7` — branch `feature/session-tabs/agent-history`, base `feature/session-tabs/dormant-tabs`
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
  its exit, before deregistration. The launch and resume paths call it with
  `<session_dir>/terminals/main.capture`.
- The jailed equivalent on `SandboxSessionManager`.
- `tddy_terminal_rpc::service::RecordedTerminalHistory` trait:
  `async fn recorded(&self, os_user: &str, session_id: &str, terminal_id: &str) -> Option<CaptureRecording>`.
  - Added to `TerminalSessionPorts` as `recorded_history`.
  - `get_terminal_history` consults it only when the store has no running terminal. It is served from
    a new module, `service/recorded_history.rs`.
- The daemon's `SessionDirRecordedHistory` (`tddy-session-lifecycle`), which answers `main` from the
  file and `None` for any other terminal id.
- The coder participant passes a port that always answers `None`.

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
| `n1` dormant-tabs | `TerminalLaunch.os_user`; the `svc_recorded_worktree.rs` metadata lookup | nothing at the code level — n1 sits below only because the line is flattened; this node resolves the session dir itself from the history request's token | change `TerminalLaunch`, the roster, or `recorded_worktree` |

## Draft PR contract

The first push publishes:

- `CaptureRecording` with its methods as `// TODO(agent-history): implement`;
- `CliSessionManager::record_main_capture`, a no-op body;
- the `RecordedTerminalHistory` trait and the `recorded_history` port field, with the daemon and coder
  implementations answering `None`;
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

### Unit / integration tests

- [ ] `packages/tddy-task/tests/capture_recording_unit.rs`:
  - a round trip keeps the offsets and bytes;
  - `replay_from` agrees with `TerminalCapture::replay_from` for the same window;
  - a wrong magic or version is refused;
  - the write is atomic (no partial file after a failed write).
- [ ] `packages/tddy-cli-sessions/tests/capture_recorder.rs`:
  - a registered `main` is written on exit before deregistration;
  - it is rewritten after the interval (with a paused tokio clock);
  - an unregistered session writes nothing.
- [ ] `packages/tddy-terminal-rpc/tests/recorded_history_acceptance.rs`:
  - with a fake port, a stopped terminal's history comes from the port;
  - a running terminal never consults it.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Format and paging | Unit over `CaptureRecording` | Pure data |
| Recorder timing | Integration with a paused tokio clock | Deterministic intervals |
| Served behaviour | Daemon acceptance through `terminal_session_service()` | The coordinate the web calls |

## Decisions & Trade-offs

- **The latest window, not a log.** It has the same bound as the live capture, so disk use is fixed
  per session and paging semantics are identical.
- **Periodic plus at-exit writes.** At-exit alone loses everything on a daemon crash; continuous
  append would need a new log format.
- **A port consulted only when nothing runs** is the designed source for a stopped terminal, not a
  fallback over a live one. A running `main` never reads the file.

## TODO

- [x] Record initial discovery (`2026-11-02-session-tabs-agent-history-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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
