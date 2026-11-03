# Changeset: A tmux-run agent survives the daemon and is re-attached

**Created:** 2026-11-03
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — client-vs-agent exit classification in the PTY cleanup path, detach-on-shutdown, resume branch, lazy re-attach in the main-terminal lookup; restart-surviving acceptance tests
**Stack:** `#tmux-agent 2/3` — **PR:** [#815](https://github.com/uppin/tddy-coder/pull/815) — branch `feature/tmux-agent/tmux-resume`, base `feature/tmux-agent/claude-in-tmux`
**PRD:** [PRD-2026-11-03-tmux-agent-resume.md](../../ft/daemon/1-WIP/PRD-2026-11-03-tmux-agent-resume.md)

## Initial Discovery

[2026-11-03-tmux-agent-tmux-resume-initial-discovery.md](./2026-11-03-tmux-agent-tmux-resume-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| PR #813 | ⚠ Prerequisite PR (inherited via the root) |
| [2026-10-24-reattach-does-not-replay-the-alternate-screen.md](../todo/2026-10-24-reattach-does-not-replay-the-alternate-screen.md) | ℹ Answered for tmux mains — the web's `SNAPSHOT` open carries tmux's alternate screen, so a re-attached client renders correctly |

## Affected Packages

- [ ] [`tddy-cli-sessions`](../../../packages/tddy-cli-sessions/README.md): shutdown detaches tmux-run mains; client-exit classification; re-attach
- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): claude resume re-attaches when the tmux session is alive
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): the `MainReattach` port implementation
- [ ] [`tddy-terminal-rpc`](../../../packages/tddy-terminal-rpc/README.md): the `MainReattach` port consulted by the stream / history / input handlers

## Responsibility

- `kill_all`: for a main terminal whose entry has `tmux_session`, cancel the client only (no `kill-session`).
- Main-exit cleanup (`spawn_terminal_cleanup`): a tmux-run main whose tmux session is still alive (`tmux_session_alive`) keeps `managed_workflows` and the exposure record; only the terminal entry is dropped and the recording finished. Agent exit (tmux session gone) → today's cleanup.
- `CliSessionManager::reattach_main(session_id, os_user, name, worktree, kind) -> Result<Arc<PtyHandle>>` — `attach_main_to_tmux` + re-register recording + exposure; returns the handle.
- Resume (claude, tmux-run): `tmux_session_alive` → `reattach_main` (pane pid re-recorded, no relaunch); else n1's fresh path.
- Lazy re-attach: a `MainReattach` port in `tddy_terminal_rpc::TerminalSessionPorts` (like the recorded-history port). The stream / history / input handlers consult it for terminal `main` only when the store has no running main, with the requester's OS user as the service resolves it from the token; the handler then asks the store again. The daemon's implementation (`tddy-session-lifecycle`, `TmuxMainReattach`) reads the session record (`tmux_session`), checks `tmux_session_alive`, and re-attaches single-flight per session (`tddy_agent_launch::claude_cli_tmux::reattach_recorded_main`, which also registers the main recording). The coder participant and the test support pass `NoMainReattach`.
- Resume of a live tmux-run session keeps the managed workflow when this daemon still has it registered (`CliSessionManager::has_managed_workflow`) and only re-attaches; a daemon with none (a restart) prepares one bound to the same `tddy-wf-<id>.sock` path the agent uses.
- Detached-agent watcher: a tmux-run main's `ClientOnly` exit starts a watcher polling `tmux_session_alive` every `DETACH_WATCH_INTERVAL` (10 s; `with_detach_watch_interval` for tests); when the tmux session is gone it drops the managed workflow (toolcall socket) and the exposure record; a re-attach (or `kill_all`) stops it.

## Boundaries

- No other harnesses (n3). No change to Delete/Signal (n1). No multi-client arbitration beyond the control lease.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` claude-in-tmux | `tddy_cli_sessions::tmux` ops (`tmux_session_alive`, `pane_pid`, `kill_tmux_session`), `attach_main_to_tmux`, `SessionMetadata.tmux_session`, claude tmux start/fresh-resume | re-attach and classification call them | change the tmux ops, the record field, the start path, or Delete |

## Draft PR contract

The first push publishes, as `// TODO(tmux-resume): implement` stubs, plus the failing tests below:

- `tddy_cli_sessions::cli_session_manager::{MainExit, classify_main_exit}` — client-vs-agent exit classification with an injected liveness probe (`tmux_reattach.rs`).
- `CliSessionManager::reattach_main` (`tmux_reattach.rs`).
- `tddy_agent_launch::claude_cli_tmux::reattach_claude_in_tmux` — the resume branch for a live tmux session; `relaunch_claude_cli` calls it instead of n1's `resume_claude_in_tmux` when `tmux_session_alive` (n1's symbols are untouched).
- `DaemonTerminalSessionStore::reattach_main_once` + `TmuxRunMain` — the single-flight re-attach entry in the main-terminal lookup. **Superseded in green** by the `MainReattach` port (Decisions).

Green wires the stubs into `spawn_terminal_cleanup`, `kill_all`, `relaunch_claude_cli` and `get_terminal`; nothing is wired in the contract so n1's behaviour is unchanged.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** after n1 is green — its tests start tmux-run sessions through n1's launcher
**Concurrent with:** none
**Blocks:** n3 `harnesses-in-tmux` (its harness resume uses the re-attach-or-fresh branch)

Real dependency edges: `n1 → n2 → n3`, `n1 → n3`

**Sequencing of the red tests:** every acceptance test in `claude_cli_tmux_reattach_acceptance.rs` starts a tmux-run session through n1's launcher, so each is red until n1's green lands (n1's start path is stubs at this commit) and only then red or green for n2's own behaviour. After n1 is green, the shutdown test and the fresh-start-after-gone test are expected to pass already (a guard on n1: `kill_all` ends the client only; the fresh path is n1's) — the tests that stay red for n2 are the re-attach, resume-live, client-exit-keeps-socket and agent-exit-cleanup ones.

## State A (after n1)

- Shutdown kills tmux-run sessions; a client exit drops the toolcall socket; Resume always starts fresh.

## State B

- The agent survives shutdown and client exits; Resume and first access re-attach to a live tmux session without restarting the agent.

## Delta

### New
- `packages/tddy-cli-sessions/src/cli_session_manager/tmux_reattach.rs`.
### Modified
- `tddy-cli-sessions` `terminals.rs` (`kill_all`), `pty_spawn.rs` (cleanup classification); `tddy-agent-launch` `claude_cli_tmux.rs`; `tddy-session-lifecycle` `terminal_session_adapter.rs`.
- Docs at wrap: `docs/ft/daemon/claude-cli-session.md` § tmux (resume/re-attach), `docs/ft/daemon/terminal-sessions.md`.

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`**: `packages/tddy-session-lifecycle/tests/claude_cli_tmux_reattach_acceptance.rs` (private tmux, stub claude, serial)
- [x] `a_daemon_shutdown_leaves_a_tmux_run_agent_running`
- [x] `a_fresh_daemon_reattaches_the_main_terminal_on_first_stream_without_restarting_the_agent`
- [x] `resume_of_a_live_tmux_run_session_reattaches_instead_of_relaunching`
- [x] `resume_of_a_tmux_run_session_whose_tmux_session_is_gone_starts_fresh`
- [x] `a_client_exit_while_tmux_lives_keeps_the_sessions_toolcall_socket`
- [x] `an_agent_exit_runs_the_main_exit_cleanup_and_records_history`

### Unit / integration tests
- [x] `tddy-cli-sessions` (`tests/tmux_reattach.rs`): cleanup classification (alive → client only, gone → agent ended, PTY-run → no probe) with an injected liveness probe — red; `kill_all` ends a tmux-run main's client but not its tmux session — a **guard, green today** (the claim is about behaviour n2 must preserve when it wires detach-on-shutdown).

## Decisions & Trade-offs

Decided by the developer 2026-11-03 (after the first green attempt stopped on three open questions):

- **Lazy re-attach via a port in the terminal service, not in the store.** `TerminalSessionStore::get_terminal` has no token and the store holds neither the OS user nor the data dir; the service has the token's OS user already. The stub `reattach_main_once` and `TmuxRunMain` are removed. The red acceptance test now streams `main` through `terminal_session_service()` rather than calling the store directly.
- **Same-daemon live resume reuses the registered workflow** (no new listener, no unlink of the agent's socket); a restarted daemon prepares a new one at the same path. Test: `resuming_a_live_session_on_the_same_daemon_keeps_its_toolcall_socket_answering`.
- **A watcher for a detached agent** (10 s poll, configurable). Test: `a_detached_agents_end_runs_the_cleanup_when_its_watcher_notices`. The recording is not finished again by the watcher: it was written at the client's exit and nothing captures the agent's output while no client is attached.
- **n1's `drain_terminals` kills no tmux session any more** (shutdown detaches main clients; tab sessions were never touched). n1's test `daemon_shutdown_kills_a_tmux_run_sessions_agent_and_tmux_session` is rewritten as `daemon_shutdown_detaches_a_tmux_run_sessions_main_and_leaves_its_agent_running`; `kill_tmux_session_at_shutdown` and `TerminalEntry.os_user` are removed. Responsibility: a PR that changes n1's shutdown contract is this one, by design.

- **Lazy re-attach in the lookup**, because a live tmux-run session reads active and the web will not offer Resume.
- **Single-flight re-attach** per session to avoid two clients from concurrent first accesses.

## TODO

- [x] Record initial discovery (`2026-11-03-tmux-agent-tmux-resume-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail) — red for n1's stubs now; see Green wave
- [x] USER REVIEW — acceptance tests — waived by the developer 2026-11-03 (reviewed with the stack at the end)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
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
- [x] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`) — scoped; see Green status
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Baseline before green

Scoped to `tddy-cli-sessions`, `tddy-agent-launch`, `tddy-session-lifecycle` (`cargo check --all-targets` and `cargo clippy --all-targets -- -D warnings` clean on the rebased tree). Red: `tmux_reattach` 3 of 4 (the classification tests; `kill_all` is a guard); the six lifecycle acceptance tests are red on n1's stubs (not run here — the test binary link is heavy; compile-checked). Not run: the rest of the workspace (CI's).

Note: the lifecycle fixtures are copied from n1's `claude_cli_tmux_session_acceptance.rs` (read-only here); the green phase may move them into `tests/support/` once n1 has landed.

## Green status (2026-11-03, first attempt — superseded by "Green status, final" below)

Rebased onto `feature/tmux-agent/claude-in-tmux` (e64891547); this PR is the two commits above it (docs, contract). Scoped runs only; the rest of the workspace is CI's.

| Gate | Result |
|---|---|
| `./dev cargo test -p tddy-cli-sessions --no-fail-fast` | all pass, incl. `tmux_reattach` 4 of 4 |
| `./dev cargo test -p tddy-agent-launch --lib` | 175 pass |
| `claude_cli_tmux_reattach_acceptance` | 5 of 6 pass; **red:** `a_fresh_daemon_reattaches_the_main_terminal_on_first_stream_without_restarting_the_agent` (lazy lookup; `reattach_main_once` is still a stub) |
| n1 guards: `claude_cli_tmux_session_acceptance`, `terminal_session_acceptance`, `tmux_tabs_acceptance` | 9/9, 12/12, 7/7 |
| `cargo clippy --all-targets -- -D warnings` | `tddy-cli-sessions`, `tddy-agent-launch` clean; `tddy-session-lifecycle` for the reattach test target only |

Implemented: `kill_all` cancels a tmux-run main's client and leaves its tmux session; `classify_main_exit` is wired into `spawn_terminal_cleanup` (the probe runs on the blocking pool; the tmux identity is captured at spawn, so a main ended by shutdown is still a client-only exit); `reattach_main` attaches the client, and returns the main already attached to that session rather than adding a second; `reattach_claude_in_tmux` reads the pane pid before attaching; `relaunch_claude_cli` re-attaches when `tmux_session_alive`, else starts fresh as before.

Checked: `ResumeSession` does not refuse a claude-cli session whose pid reads active (`resume_claude_cli_session` has no pid check), so the live-tmux case goes to re-attach.

Deviations from the Responsibility section:
- `reattach_main` does not register the main recording or the exposure. The recording path needs the daemon's data dir, which the manager does not hold, so callers register it as after a start (`record_main_terminal`). A client's exit keeps the exposure record rather than re-registering it: a restarted daemon has no metadata to rebuild it from, because `stack_parent` is not persisted.
- `TerminalEntry.os_user` is removed; its only reader was `kill_all`'s tmux kill.
- Fixture: `a_claude_cli_start_request` now sets `managed_codebase: true` and `recipe: "tdd"`. Without them the session is unmanaged, has no toolcall listener, and `a_client_exit_while_tmux_lives_keeps_the_sessions_toolcall_socket` asserted on a socket that never existed, while `an_agent_exit_...` could only pass vacuously. Assertions are unchanged.

Open decisions (not implemented; this node stops here):
1. **Lazy re-attach (`reattach_main_once`).** The Responsibility section says the lookup reads the tmux run's details "from its metadata", but `SessionMetadata` holds neither the run's OS user nor the daemon data dir, and `DaemonTerminalSessionStore` holds only the CLI and sandbox managers. `get_terminal` gets no token, so nothing else supplies the OS user. Needed: where the OS user comes from (persist it at start, or a host-side resolver), and the data dir for the recording path and the metadata read.
2. **A live same-daemon resume deletes the agent's toolcall socket.** Verified with a temporary test: detach, then `ResumeSession` on the same daemon. The client exit keeps the managed workflow (`ClientOnly`); the resume's `prepare_managed_workflow` binds a new `tddy-wf-<id>.sock`; `attach_managed_workflow` then drops the old listener, whose `Drop` unlinks that path. The socket is gone after the resume. Options: release the old workflow before the new listener binds (a window with no socket for the live agent), or skip creating a new workflow when the tmux session is alive (needs a probe before `prepare_managed_workflow`). The restart test does not catch it, since its manager is empty.
3. **No observer for a detached agent that ends.** After a `ClientOnly` exit with no client attached, nothing sees the tmux session end, so the `AgentEnded` cleanup (socket, exposure) waits for the next attach. The PRD says the cleanup runs "when the agent's tmux session has ended". Needs a watcher: owner and poll interval.

## Green status, final (2026-11-03)

Rebased onto the wrapped `feature/tmux-agent/claude-in-tmux` (621f85af7). Scoped runs only; the rest of the workspace is CI's.

| Gate | Result |
|---|---|
| `cargo test -p tddy-cli-sessions -p tddy-terminal-rpc` | all pass |
| `cargo test -p tddy-agent-launch --lib` | 175 pass |
| `claude_cli_tmux_reattach_acceptance` | 8 of 8 (the six originals + same-daemon resume socket + detached watcher) |
| `claude_cli_tmux_session_acceptance`, `tmux_tabs_acceptance`, `terminal_session_acceptance` | 12/12, 12/12, 8/8 (rebased onto n1 0e251e0a0) |
| `tddy-coder --test two_server_parity_acceptance` | 12/12 |
| `cargo clippy --all-targets -- -D warnings`, `cargo fmt` | `tddy-cli-sessions`, `tddy-terminal-rpc`, `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-coder` clean |

The three open decisions of the first attempt are resolved as listed under Decisions & Trade-offs. Known limit: a watcher is started after a daemon shutdown's `kill_all` cancels the main clients (they exit as `ClientOnly`); it dies with the process.

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
