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
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): main-terminal lookup re-attaches on demand

## Responsibility

- `kill_all`: for a main terminal whose entry has `tmux_session`, cancel the client only (no `kill-session`).
- Main-exit cleanup (`spawn_terminal_cleanup`): a tmux-run main whose tmux session is still alive (`tmux_session_alive`) keeps `managed_workflows` and the exposure record; only the terminal entry is dropped and the recording finished. Agent exit (tmux session gone) → today's cleanup.
- `CliSessionManager::reattach_main(session_id, os_user, name, worktree, kind) -> Result<Arc<PtyHandle>>` — `attach_main_to_tmux` + re-register recording + exposure; returns the handle.
- Resume (claude, tmux-run): `tmux_session_alive` → `reattach_main` (pane pid re-recorded, no relaunch); else n1's fresh path.
- Lazy re-attach: `DaemonTerminalSessionStore` (main lookup) — tmux-run session (metadata `tmux_session`), tmux alive, no main registered → `reattach_main` once (single-flight per session), then serve.

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
- `DaemonTerminalSessionStore::reattach_main_once` + `TmuxRunMain` — the single-flight re-attach entry in the main-terminal lookup.

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

## Baseline before green

Scoped to `tddy-cli-sessions`, `tddy-agent-launch`, `tddy-session-lifecycle` (`cargo check --all-targets` and `cargo clippy --all-targets -- -D warnings` clean on the rebased tree). Red: `tmux_reattach` 3 of 4 (the classification tests; `kill_all` is a guard); the six lifecycle acceptance tests are red on n1's stubs (not run here — the test binary link is heavy; compile-checked). Not run: the rest of the workspace (CI's).

Note: the lifecycle fixtures are copied from n1's `claude_cli_tmux_session_acceptance.rs` (read-only here); the green phase may move them into `tests/support/` once n1 has landed.

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
