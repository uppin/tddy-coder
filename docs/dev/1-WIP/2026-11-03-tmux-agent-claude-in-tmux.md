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

- [x] [`tddy-service`](../../../packages/tddy-service/README.md): `StartSessionRequest.run_in_tmux = 47`
- [x] [`tddy-changeset`](../../../packages/tddy-changeset/README.md): `SessionMetadata.tmux_session`
- [x] [`tddy-cli-sessions`](../../../packages/tddy-cli-sessions/README.md): `tmux` module (moved helpers + agent-session ops), main terminal attached to a tmux session
- [x] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): claude start/resume in tmux, delete kills the tmux session, request validation
- [x] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): tmux tab code imports the moved helpers; shutdown kills tmux-run sessions' tmux sessions
- [x] [`tddy-session-activity`](../../../packages/tddy-session-activity/README.md): delete kills the recorded tmux session
- [x] [`tddy-web`](../../../packages/tddy-web/README.md): Create-pane checkbox and request field

## Responsibility

- Proto `StartSessionRequest.run_in_tmux` (bool, field 47) + regenerated TS.
- `SessionMetadata.tmux_session: Option<String>` (`skip_serializing_if = "Option::is_none"`).
- `tddy_cli_sessions::tmux` (new module): moved from `tddy-session-lifecycle/.../tmux_host.rs` — `TMUX_BINARY_ENV`, `tmux_binary`, `tmux_attach_argv`, `tmux_session_name`, `tmux_new_session_argv`, `parse_list_sessions`, `ensure_reachable`, `list_tmux_sessions(os_user)`; new: `agent_tmux_session_name(session_id) -> String` (`tddy-<full session id>`), `start_agent_tmux_session(os_user, name, worktree, env, argv) -> Result<u32 /*pane pid*/, TmuxError>`, `kill_tmux_session(os_user, name)`, `tmux_session_alive(os_user, name) -> bool`, `pane_pid(os_user, name)`.
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
- [x] `tddy-cli-sessions` `tmux` unit tests: `agent_tmux_session_name` (full session id, valid name), `start_agent_tmux_session` argv shape (`new-session -d -s -c -e K=V -- argv`), moved helper tests move with the code.
- [x] `tddy-changeset`: a `.session.yaml` with `tmux_session` round-trips; one without it reads `None`.
- [x] Existing `tmux_tabs_acceptance` / `tmux_new_session_acceptance` stay green after the move (regression guard).

## Decisions & Trade-offs

- **Pane pid as `pid`** — liveness/Signal/Delete keep their code paths and act on the agent, not the client.
- **Env via `new-session -e`** — tmux panes inherit the server's env, not the client's.
- **n1 keeps "terminate kills tmux" on shutdown** so it is correct merged alone; n2 flips it to detach.
- **Helper move folded into this node** (developer, 2026-11-03) instead of a restructure-engine Refactor node.
- **`deny_unknown_fields` compatibility**: older daemons cannot read a tmux-run record — documented, accepted.
- **Name** (developer, 2026-11-03): `tddy-` + the full session id; `:` and `.` become `_` (tmux's own substitution). A prefix of a UUIDv7 repeats every ~65 s, so a prefix could collide; the full id cannot, and the name is derivable from the id alone.
- **Delete with unreadable metadata** (developer, 2026-11-03): the name is authoritative, so Delete kills `tddy-<session id>` as the session's OS user and treats "no such session" as fine. A tmux that cannot be run at all is logged and does not block deleting the broken session (a recorded name that cannot be killed still refuses the delete). Not a fallback: the derived name is the name.
- **`run_in_tmux` is offered to the agent-facing StartSession tool** (developer, 2026-11-03): a boolean beside `sandbox`, forwarded to `StartSessionRequest.run_in_tmux`; the web parity fixture lists it.
- **Refusals** (developer, 2026-11-03): `run_in_tmux` on claude-cli with `sandbox`, `sandboxed_codebase` or a split placement (`codebase_daemon_instance_id`) is `invalid_argument` naming the conflict, in `refuse_a_conflicting_tmux_run_start`, called from `start_session_core_cancellable` after the harness profile, before routing.
- **SIGHUP-ignoring agent**: Delete kills the tmux session, then `delete_session_directory` terminates the recorded pane pid (SIGTERM, then SIGKILL), so an agent that traps HUP is gone; pinned by an acceptance case.
- **Pane environment**: the pane gets the target user's `HOME` and configured `PATH` as the PTY runtime gives them, with `run_in_tmux`'s env on top, via `new-session -e`.
- **`pane_pid` target**: `=<name>:`, since a bare `=<name>` prints nothing on tmux 3.6.
- **Absent session**: a kill treats "no server running", "can't find session", "session not found" and "error connecting … no such file" as already gone. An attach failure kills the new tmux session it started.
- **Shutdown**: `kill_all` kills the tmux sessions of **main** terminals first (a tmux-run agent's; never a tab's, which is the user's), as the `os_user` recorded on `TerminalEntry`, and logs and continues on failure.
- **Delete**: handler side, in `tddy-agent-launch`, before teardown, because `tddy-session-activity` cannot depend on `tddy-cli-sessions`. A refused kill refuses the delete with the session intact.
- **Other session types** ignore `run_in_tmux` until 3/3.
- **Argv**: `CliSessionManager::with_claude_launch_flags` (over `build_claude_argv`) is shared by the PTY and tmux launches.
- **Resume**: `resume_claude_in_tmux` always passes `--resume`, whatever the caller set.
- **Main terminal model**: `""`, because the contract has no model parameter for the tmux client.
- **Web**: the checkbox is hidden on the project-home (`placementFixed`) variant, whose start spec does not carry it. Switching the sandbox on clears `runInTmux`, and the request never carries both.

## TODO

- [x] Record initial discovery (`2026-11-03-tmux-agent-claude-in-tmux-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — waived by the developer 2026-11-03 (reviewed with the stack at the end)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [x] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [x] Validate changes (/validate-changes)
- [x] Refactor issues from change validation
- [x] USER REVIEW — development complete
- [x] Validate tests (/validate-tests)
- [x] Refactor test issues
- [x] Validate production readiness (/validate-prod-ready)
- [x] Refactor production readiness issues
- [x] Analyze code quality (/analyze-clean-code)
- [x] Refactor code quality issues
- [x] Final validation (/validate-changes)
- [x] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [x] USER REVIEW — scope developed, every gap approved: ready to wrap

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

## Green status

Green was run on the branch based on `origin/master` (`2e4af52ac`, with #813 merged). Scoped to the touched packages; whole-workspace health is CI's.

| Gate | Result |
|---|---|
| `./dev cargo test -p tddy-cli-sessions -p tddy-agent-launch -p tddy-tool-engine --lib` | 23 + 175 + 13 passed, 0 failed (agent-launch includes the 4 new `start_request_checks` unit tests; cli-sessions the full-id name test) |
| `./dev cargo test -p tddy-session-lifecycle --test claude_cli_tmux_session_acceptance` | 9 passed, 0 failed (adds: Delete ends an agent that ignores SIGHUP; Delete with unreadable metadata kills the tmux session) |
| `./dev cargo test -p tddy-tool-engine --test start_session_args_parity --test start_session_schema` | 4 + 10 passed, 0 failed |
| Earlier on this branch: `tddy-cli-sessions` + `tddy-changeset` 243/243, `tmux_tabs` 7/7, `tmux_new_session` 5/5, `terminal_session` 12/12, `svc_terminal_ports` 4/4, bun `src/components/sessions` 440 | passed |
| `./dev cargo clippy --all-targets -p tddy-agent-launch -p tddy-cli-sessions -p tddy-tool-engine -p tddy-session-lifecycle -- -D warnings`; `cargo fmt --check` on the same | clean |
| Cypress `CreateSessionRunInTmuxAcceptance.cy.tsx`, `createSessionRequestParity.cy.ts` | 3/3, 2/2 passing |

Every decision that was open at the first green is resolved in Decisions & Trade-offs above (tool offers `run_in_tmux`; full-id name, FIXME removed; refusals for `sandboxed_codebase` and split placement; Delete with unreadable metadata; SIGHUP-ignoring agent verified by a stub that traps HUP).

## Refactoring Needed
### From /validate-changes
- ✅ **CRITICAL fixed** — `kill_all` killed the tmux session of **every** terminal naming one, tmux tabs included: daemon shutdown destroyed the user's own tmux sessions a tab was attached to (Boundaries: "no change to tmux tabs' behaviour"). Now main terminals only (`drain_terminals`); pinned by `tmux_tabs_acceptance::daemon_shutdown_detaches_a_tmux_tab_and_leaves_its_tmux_session_running` (red first) and `claude_cli_tmux_session_acceptance::daemon_shutdown_kills_a_tmux_run_sessions_agent_and_tmux_session`.
- ✅ **WARNING fixed** — the web disabled the tmux checkbox only for the sandbox; the daemon also refuses `sandboxed_codebase` and split. `runInTmuxUnavailable` (hook) disables and clears it for all three; the request never carries `run_in_tmux` with any of them (`CreateSessionRequestBuilder.cy.ts`, 2 cases).
- ⚠️ **WARNING, not fixed (reverses a developer decision)** — the pane env (provider `ANTHROPIC_AUTH_TOKEN` included) is on `tmux new-session -e`'s argv, readable by local users while that client runs. Recorded: `docs/dev/todo/2026-11-03-tmux-agent-env-on-the-tmux-argv.md`.
- ✅ `tmux_session_alive` had no test (n2 consumes it): `a_tmux_session_reads_alive_while_it_runs_and_not_alive_once_killed`.
### From /validate-tests
- ✅ `a_session_yaml_without_a_tmux_session_reads_none_and_writes_none` never read anything back; now writes and reads.
### From /validate-prod-ready
- ✅ `#[allow(clippy::too_many_arguments)]` added on the 9-argument `build_claude_cli_argv`: replaced by `with_claude_launch_flags(argv, initial_prompt, append_system_prompt_file, extra_args)`; no suppression added by this PR.
- No `TODO`/`FIXME`/`unimplemented!` left in this PR's code; no debug output.
### From /analyze-clean-code
- ✅ `spawn_claude_cli_process` 42 → 73 → **52** lines: one `ClaudeCliTmuxLaunch`, the PTY branch in `start_claude_in_pty`.
- ✅ `kill_all` 84 (base) → 101 → **83**: the drain in `drain_terminals`.
- ✅ `start_session_on_request` (code issue `complexity-svc-start-session-core`): the refusal moved to `start_session_core_cancellable`; `svc_start_session_core.rs` is unchanged from base (it had reached 500 production lines).
- ✅ `resume_claude_cli_session` (deferred code issue): green grew it 142 → 147; now **141** (`..Default::default()` launch, one relaunch call). Record updated.
- ⚠️ `CreateSessionPane.tsx` 502 → 509 production lines (already over 500): **not split** — stack-shared with #815/#816; record `oversized-file-create-session-pane` updated (regressed).

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | 2026-11-03 | ✅ stack gate clean (base `master`, already current at `2e4af52ac`, 6 own commits); 1 critical + 2 warnings fixed, 1 warning recorded |
| /validate-tests | 2026-11-03 | ✅ one description/body mismatch fixed; new tests Given/When/Then |
| /validate-prod-ready | 2026-11-03 | ✅ suppression removed; no markers |
| /analyze-clean-code | 2026-11-03 | ✅ two functions back under/at base; one stack-shared file recorded |
| file-length gate | 2026-11-03 | ⚠️ `CreateSessionPane.tsx` 502 → 509 (stack-shared, recorded); `gen/session_pb.ts` and `agent_activity_unit_tests.rs` are generated / test code |
