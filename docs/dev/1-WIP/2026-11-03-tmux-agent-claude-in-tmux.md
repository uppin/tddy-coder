# Changeset: A Claude CLI session can run its agent inside tmux

**Created:** 2026-11-03
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — proto + `tddy-changeset` + `tddy-cli-sessions` (new tmux module, moved helpers, main-in-tmux start) + `tddy-agent-launch` (claude start/resume/delete) + `tddy-session-lifecycle` (import move, shutdown) + web; real-tmux acceptance tests; pid semantics change for tmux-run sessions
**Stack:** `#tmux-agent 1/3` — branch `feature/tmux-agent/claude-in-tmux`, base `master` — **PR:** [#814](https://github.com/uppin/tddy-coder/pull/814)
**PRD:** [PRD-2026-11-03-claude-cli-in-tmux.md](../../ft/daemon/1-WIP/PRD-2026-11-03-claude-cli-in-tmux.md)

## Initial Discovery

[2026-11-03-tmux-agent-claude-in-tmux-initial-discovery.md](./2026-11-03-tmux-agent-claude-in-tmux-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| PR #813 (`fix/session-tabs-sweep`, open) | ⚠ **Prerequisite PR** — it changes `tddy-terminal-rpc/src/pty_runtime.rs` (the PTY spawn now refuses unreachable users with the kernel's plan) and the tmux privilege-drop TODO. This node spawns the tmux client through that path; rebase onto master once #813 lands |
| [2026-11-02-tmux-listing-and-tab-use-two-privilege-drops.md](../todo/2026-11-02-tmux-listing-and-tab-use-two-privilege-drops.md) | ⚠ During — `new-session`/`kill-session`/`display-message` run via `run_output_as_user` (pre_exec drop) while the client runs through the PTY (setpriv); same split as the tabs; do not widen it |
| [2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md](../todo/2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md) | ⚠ During — on a supervised install a tmux-run session for another user is refused with `CannotBecomeUser` rather than run as the daemon user |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-claude-cli-session-resume-claude-cli-session.md` | ⚠ During — deferred by the developer; the tmux branch of `resume_claude_cli_session` goes in a helper so the function does not grow |

## Affected Packages

- [ ] [`tddy-service`](../../../packages/tddy-service/README.md): `StartSessionRequest.run_in_tmux = 47`
- [ ] [`tddy-changeset`](../../../packages/tddy-changeset/README.md): `SessionMetadata.tmux_session`
- [ ] [`tddy-cli-sessions`](../../../packages/tddy-cli-sessions/README.md): `tmux` module (moved helpers + agent-session ops), main terminal attached to a tmux session
- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): claude start/resume in tmux, delete kills the tmux session, request validation
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): tmux tab code imports the moved helpers; shutdown kills tmux-run sessions' tmux sessions
- [ ] [`tddy-session-activity`](../../../packages/tddy-session-activity/README.md): delete kills the recorded tmux session
- [ ] [`tddy-web`](../../../packages/tddy-web/README.md): Create-pane checkbox and request field

## Responsibility

- Proto `StartSessionRequest.run_in_tmux` (bool, field 47) + regenerated TS.
- `SessionMetadata.tmux_session: Option<String>` (`skip_serializing_if = "Option::is_none"`).
- `tddy_cli_sessions::tmux` (new module): moved from `tddy-session-lifecycle/.../tmux_host.rs` — `TMUX_BINARY_ENV`, `tmux_binary`, `tmux_attach_argv`, `tmux_session_name`, `tmux_new_session_argv`, `parse_list_sessions`, `ensure_reachable`, `list_tmux_sessions(os_user)`; new: `agent_tmux_session_name(session_id) -> String` (`tddy-<short id>`), `start_agent_tmux_session(os_user, name, worktree, env, argv) -> Result<u32 /*pane pid*/, TmuxError>`, `kill_tmux_session(os_user, name)`, `tmux_session_alive(os_user, name) -> bool`, `pane_pid(os_user, name)`.
- `CliSessionManager::attach_main_to_tmux(session_id, worktree, os_user, kind, name) -> Result<Arc<PtyHandle>>` — the main terminal as a tmux client (`MAIN_TERMINAL_ID`, `tmux_session = Some(name)`), with the main recording hook.
- `tddy-agent-launch`: claude start with `run_in_tmux` → tmux session + attach, `pid` = pane pid, `tmux_session` recorded; claude resume of a tmux-run session → kill leftover, fresh tmux session with the resume argv; `run_in_tmux && sandbox` → `invalid_argument`; non-claude types ignore it (until n3).
- Delete (`session_deletion`): when `tmux_session` is recorded, also `kill_tmux_session`.
- Daemon shutdown (`kill_all` path): tmux-run main terminals' tmux sessions are killed (n2 changes this).
- `tddy-session-lifecycle` `svc_terminal_ports`: tmux tab code uses `tddy_cli_sessions::tmux` (no behaviour change); `tmux_host.rs` keeps only `HostTmux` (the `TmuxHost` port impl).
- Web: `useCreateSessionPlacement` `runInTmux` state, `CreateSessionClaudeCliFields` checkbox `create-session-tmux-toggle` (disabled with sandbox), `createSessionRequest.ts` `runInTmux` → `run_in_tmux` for claude.

## Boundaries

- No re-attach to a live tmux session, no detach-on-shutdown, no client-vs-agent exit classification (n2 `tmux-resume`).
- No cursor/opencode/muse/warp tmux mode (n3 `harnesses-in-tmux`).
- No change to tmux tabs' behaviour, sandboxed sessions, or non-tmux sessions.
- No unification of the two privilege-drop mechanisms (deferred TODO).

## Dependencies

None — root node (depends on `master`; prerequisite PR #813 should land first, see Prerequisites).

## Draft PR contract

The first push publishes: the proto field (+ TS), `SessionMetadata.tmux_session`, the `tddy_cli_sessions::tmux` module with moved helpers (moved for real — a move, green) and the new agent-session functions as `// TODO(claude-in-tmux): implement`, `attach_main_to_tmux` stub, the web state/prop surface, and the failing tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — daemon tests drive this daemon's own launcher with a stub claude and a private tmux (`TMUX_TMPDIR`); web tests use the in-memory backend
**Concurrent with:** none
**Blocks:** n2 `tmux-resume` (re-attach builds on the tmux session ops and `attach_main_to_tmux`), n3 `harnesses-in-tmux`

Real dependency edges: `n1 → n2 → n3`, `n1 → n3`

## State A

- No way to run the agent in tmux. tmux helpers are `pub(super)` in `tddy-session-lifecycle` (`tmux_host.rs`), unreachable from `tddy-agent-launch` (crate cycle). The recorded `pid` is always the PTY child; Delete/Signal act on it; shutdown `kill_all`s PTY children.

## State B

- A claude-cli session created with `run_in_tmux` runs Claude in tmux as the session's OS user; the main terminal is a tmux client; `pid` is the pane pid; Delete kills agent + tmux session; Resume starts fresh in tmux; shutdown kills it.

## Delta

### New
- `packages/tddy-cli-sessions/src/tmux.rs` (moved helpers + agent-session ops), `packages/tddy-cli-sessions/src/cli_session_manager/tmux_main.rs` (`attach_main_to_tmux`).
- `packages/tddy-agent-launch/src/claude_cli_tmux.rs` (start/resume tmux branch).
### Modified
- `session.proto`, generated TS; `session_metadata.rs` (+ production literals via `..`); `tddy-agent-launch` start dispatch/validation, `claude_cli_spawn*.rs`, `svc_resume_claude_cli_session.rs` (one call into the helper); `tddy-session-activity` `session_deletion.rs`; `tddy-cli-sessions` `kill_all`; `tddy-session-lifecycle` `svc_terminal_ports.rs` + `tmux_host.rs` (imports); web Create pane files.
- Docs at wrap: `docs/ft/daemon/claude-cli-session.md` § tmux, `docs/ft/daemon/terminal-sessions.md`, `docs/ft/web/session-drawer.md`.

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`**: `packages/tddy-session-lifecycle/tests/claude_cli_tmux_session_acceptance.rs` (private tmux via `TMUX_TMPDIR`, identity → current OS user, stub claude recording argv/env/cwd, serial)
- [x] `a_claude_session_started_in_tmux_runs_claude_in_a_named_tmux_session_in_the_worktree`
- [x] `the_main_terminal_of_a_tmux_run_session_is_attached_to_its_tmux_session`
- [x] `a_tmux_run_session_records_the_pane_pid_and_reads_active`
- [x] `claude_in_tmux_sees_the_session_environment`
- [x] `deleting_a_tmux_run_session_kills_the_agent_and_the_tmux_session`
- [x] `resuming_a_terminated_tmux_run_session_starts_a_fresh_tmux_session_with_the_resume_argv`
- [x] `run_in_tmux_with_sandbox_is_refused`

**`tddy-web`**: `packages/tddy-web/cypress/component/CreateSessionRunInTmuxAcceptance.cy.tsx`
- [x] `the Create pane offers "Run the agent in tmux" for a Claude CLI session`
- [x] `checking it sends run_in_tmux on StartSession`
- [x] `the option is disabled while the session is sandboxed`

### Unit / integration tests
- [x] `tddy-cli-sessions` `tmux` unit tests: `agent_tmux_session_name` (short id, valid name), `start_agent_tmux_session` argv shape (`new-session -d -s -c -e K=V -- argv`), moved helper tests move with the code.
- [x] `tddy-changeset`: a `.session.yaml` with `tmux_session` round-trips; one without it reads `None`.
- [x] Existing `tmux_tabs_acceptance` / `tmux_new_session_acceptance` stay green after the move (regression guard).

## Decisions & Trade-offs

- **Pane pid as `pid`** — liveness/Signal/Delete keep their code paths and act on the agent, not the client.
- **Env via `new-session -e`** — tmux panes inherit the server's env, not the client's.
- **n1 keeps "terminate kills tmux" on shutdown** so it is correct merged alone; n2 flips it to detach.
- **Helper move folded into this node** (developer, 2026-11-03) instead of a restructure-engine Refactor node.
- **`deny_unknown_fields` compatibility**: older daemons cannot read a tmux-run record — documented, accepted.

## TODO

- [x] Record initial discovery (`2026-11-03-tmux-agent-claude-in-tmux-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
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

Scoped to the touched packages (`tddy-changeset`, `tddy-cli-sessions`, `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-web`); whole-workspace health is CI's.

**Surface published (builds, `cargo clippy -p <pkg> --all-targets -- -D warnings` clean):** proto `run_in_tmux = 47` + regenerated TS (all four gen dirs `check` clean); `SessionMetadata.tmux_session`; `tddy_cli_sessions::tmux` (helpers moved for real from `tmux_host.rs`, which keeps only `HostTmux`; new `agent_tmux_session_name`, `agent_new_session_args`, `start_agent_tmux_session`, `kill_tmux_session`, `tmux_session_alive`, `pane_pid` as `TODO(claude-in-tmux)` stubs); `CliSessionManager::attach_main_to_tmux` stub; `tddy_agent_launch::claude_cli_tmux` (`start_claude_in_tmux`, `resume_claude_in_tmux`, `relaunch_claude_cli`) wired into the claude start and resume paths; web `runInTmux` state/prop/request plumbing (no checkbox yet).

**Red (fail because the implementation is missing):**

| Test | Failure |
|---|---|
| `tddy-session-lifecycle/tests/claude_cli_tmux_session_acceptance.rs` — 6 of 7 (`a_claude_session_started_in_tmux…`, `the_main_terminal…`, `a_tmux_run_session_records…`, `claude_in_tmux_sees…`, `deleting_a_tmux_run_session…`, `resuming_a_terminated…`) | panic in the `start_claude_in_tmux` stub |
| `…::run_in_tmux_with_sandbox_is_refused` | no `invalid_argument` refusal: the start proceeds into the sandbox path (which here dies on a missing `tddy-sandbox-runner`, a harness gap, not the cause) |
| `tddy-cli-sessions` `tmux::tests` — 3 (`an_agent_tmux_session_is_named_after_the_short_session_id`, `an_agent_tmux_session_name_is_a_valid_new_session_name`, `an_agent_session_starts_detached_in_the_worktree_with_its_environment_and_argv`) | `unimplemented!` stubs |
| `tddy-web/cypress/component/CreateSessionRunInTmuxAcceptance.cy.tsx` — 3 of 3 | `create-session-tmux-toggle` not rendered yet |

**Green (guards):** the 10 moved `tmux` helper tests in `tddy-cli-sessions`; `tmux_tabs_acceptance` (7) and `tmux_new_session_acceptance` (5) in `tddy-session-lifecycle`; the two new `tddy-changeset` `tmux_session` tests (the field is data, so they pass on the surface alone; 30 of 30 `session_metadata` tests pass).

**Pre-existing failures:** none seen in the targeted suites. Not run locally: the rest of the packages' suites.

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
