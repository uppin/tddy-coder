# Changeset: A tmux-run agent survives the daemon and is re-attached

**Created:** 2026-11-03
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — client-vs-agent exit classification in the PTY cleanup path, detach-on-shutdown, resume branch, lazy re-attach in the main-terminal lookup; restart-surviving acceptance tests
**Stack:** `#tmux-agent 2/3` — branch `feature/tmux-agent/tmux-resume`, base `feature/tmux-agent/claude-in-tmux`
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

The first push publishes `reattach_main` and the classification helper as `// TODO(tmux-resume): implement`, plus the failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** after n1 is green — its tests start tmux-run sessions through n1's launcher
**Concurrent with:** none
**Blocks:** n3 `harnesses-in-tmux` (its harness resume uses the re-attach-or-fresh branch)

Real dependency edges: `n1 → n2 → n3`, `n1 → n3`

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
- [ ] `a_daemon_shutdown_leaves_a_tmux_run_agent_running`
- [ ] `a_fresh_daemon_reattaches_the_main_terminal_on_first_stream_without_restarting_the_agent`
- [ ] `resume_of_a_live_tmux_run_session_reattaches_instead_of_relaunching`
- [ ] `resume_of_a_tmux_run_session_whose_tmux_session_is_gone_starts_fresh`
- [ ] `a_client_exit_while_tmux_lives_keeps_the_sessions_toolcall_socket`
- [ ] `an_agent_exit_runs_the_main_exit_cleanup_and_records_history`

### Unit / integration tests
- [ ] `tddy-cli-sessions`: cleanup classification (alive → keep, gone → clean) with an injected liveness probe; `kill_all` does not kill a tmux-run main's tmux session.

## Decisions & Trade-offs

- **Lazy re-attach in the lookup**, because a live tmux-run session reads active and the web will not offer Resume.
- **Single-flight re-attach** per session to avoid two clients from concurrent first accesses.

## TODO

- [x] Record initial discovery (`2026-11-03-tmux-agent-tmux-resume-initial-discovery.md`)
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
