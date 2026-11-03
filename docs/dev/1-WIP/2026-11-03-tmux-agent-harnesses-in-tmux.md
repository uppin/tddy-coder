# Changeset: Every CLI harness session can run its agent in tmux

**Created:** 2026-11-03
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — four harnesses' start/resume, cursor's identity switch in tmux mode, exit-hook source for tmux-run mains (remain-on-exit + capture-pane for Warp's token), web request/pane changes
**Stack:** `#tmux-agent 3/3` — **PR:** [#816](https://github.com/uppin/tddy-coder/pull/816) — branch `feature/tmux-agent/harnesses-in-tmux`, base `feature/tmux-agent/tmux-resume`
**PRD:** [PRD-2026-11-03-harnesses-in-tmux.md](../../ft/daemon/1-WIP/PRD-2026-11-03-harnesses-in-tmux.md)

## Initial Discovery

[2026-11-03-tmux-agent-harnesses-in-tmux-initial-discovery.md](./2026-11-03-tmux-agent-harnesses-in-tmux-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| PR #813 | ⚠ Prerequisite PR (inherited via the root) |
| `packages/tddy-agent-launch/docs/code-issues/crap-svc-start-sandboxed-cursor-cli-session.md` | — Unrelated: sandboxed cursor is out of scope (host only) |

## Affected Packages

- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): cursor + generic harness start/resume in tmux
- [ ] [`tddy-cli-sessions`](../../../packages/tddy-cli-sessions/README.md): exit hook for tmux-run mains runs on agent exit with the pane's final screen
- [ ] [`tddy-web`](../../../packages/tddy-web/README.md): checkbox for every CLI harness; request carries it

## Responsibility

- `start_agent_tmux_session` gains `remain_on_exit: bool` (set for harnesses with an exit hook, i.e. warp; harmless for others).
- Exit-hook source: for a tmux-run main with an `ExitHook`, the hook runs when the agent exits (pane dead, `remain-on-exit`), with `tmux capture-pane -p -S -` of that pane as its output; then `kill_tmux_session`. A client exit never runs it.
- cursor-cli: `run_in_tmux` → tmux start as the session's OS user with the chat id argv; resume via n2's re-attach-or-fresh with `--resume <chat>`.
- generic harness (opencode/muse/warp): `launch_cli_harness` tmux path (as os user, extras env via `-e`, exit hook per above); resume via re-attach-or-fresh with the harness resume argv (warp `--resume <token>`).
- `run_in_tmux` stops being ignored for these types; still refused with `sandbox`.
- Web: checkbox in the Create pane for every CLI harness type; `createSessionRequest.ts` stops clearing it for opencode/muse/warp and sends it for cursor.

## Boundaries

- No tool/workspace sessions, no sandboxed sessions, no change to cursor's daemon-user identity outside tmux.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` claude-in-tmux | tmux ops, `attach_main_to_tmux`, record field, proto field, web checkbox (claude) | calls them for the other harnesses; extends the checkbox to more types | change their signatures beyond the new `remain_on_exit` parameter |
| `n2` tmux-resume | `reattach_main`, client-vs-agent exit classification, lazy re-attach | harness resume uses re-attach-or-fresh; exit hooks hang off the agent-exit branch | change re-attach or shutdown behaviour |

## Draft PR contract

The first push publishes the `remain_on_exit` parameter, the tmux branches of the cursor/generic launchers as `// TODO(harnesses-in-tmux): implement`, and the failing tests below.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** after n2 is green — resume tests need re-attach
**Concurrent with:** none
**Blocks:** nothing

Real dependency edges: `n1 → n2 → n3`, `n1 → n3`

## State A (after n2)

- Only claude-cli can run in tmux; other types ignore `run_in_tmux`; exit hooks fire on client exit.

## State B

- All CLI harness sessions can run in tmux, as the session's OS user; Warp's token survives via the final screen.

## Delta

### Modified
- `tddy-agent-launch` cursor start/resume, `opencode_cli_spawn.rs` (generic), start dispatch validation; `tddy-cli-sessions` `tmux.rs`, `pty_spawn.rs` (hook on agent exit); web Create pane + `createSessionRequest.ts`.
- Docs at wrap: the four harness session docs § tmux.

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`**: `packages/tddy-session-lifecycle/tests/cli_harness_tmux_acceptance.rs` (private tmux, stub harness binaries via `cli_harnesses.<type>.binary_path`, serial)
- [x] `each_cli_harness_starts_in_tmux_as_the_session_os_user`
- [x] `each_cli_harness_resumes_in_a_fresh_tmux_session_with_its_resume_argv`
- [x] `a_live_tmux_run_harness_session_is_reattached_on_resume`
- [x] `a_warp_session_in_tmux_records_its_resume_token_from_the_final_screen`
- [x] `a_client_exit_does_not_run_a_harness_exit_hook`
- [x] 🆕 `a_fresh_daemon_reattaches_a_live_harness_session_on_first_stream_without_restarting_it` (opencode, warp — same pane pid, the main keeps its kind)
- [x] 🆕 `a_warp_agent_that_exits_while_detached_after_a_restart_records_its_resume_token`

**`tddy-web`**: `packages/tddy-web/cypress/component/CreateSessionRunInTmuxAcceptance.cy.tsx` (extended)
- [x] `the Create pane offers "Run the agent in tmux" for every CLI harness type`
- [x] `opencode, muse and warp send run_in_tmux when checked` (one case per type: `opencode-cli|muse-cli|warp-cli sends run_in_tmux when checked`)
- [x] 🆕 `a sandbox left on from claude-cli does not disable the option for opencode-cli`

### Unit / integration tests
- [x] `tddy-cli-sessions`: `remain_on_exit_args` / `capture_pane_args` arg shape (unit, `tmux.rs`); `exit_hook_capture` reads the pane on agent exit only (`tests/tmux_exit_hook.rs`) — red on stubs.

## Decisions & Trade-offs

- **`remain-on-exit` + `capture-pane`** for the exit hook — the pane's final screen is the only place Warp's banner survives once the pane's process exits.
- **cursor as the session's OS user in tmux mode only** (developer, 2026-11-03).
- **Harness re-attach and exit-watch parity** (developer, 2026-11-03, sent back to /green from /validate-changes): the lazy re-attach after a daemon restart (n2's `MainReattach` port → `claude_cli_tmux::reattach_recorded_main`) dispatches by the recorded `session_type` — claude-cli as before, every other CLI harness through `cli_harness_tmux::reattach_recorded_harness` with the session's own terminal kind — and a re-attached opencode/muse/warp agent is watched for its exit again (its resume-token recorder, armed at once), so Warp's token is recorded even when the agent exits with no client attached. A pane found already dead runs the hook on its final screen and attaches nothing. A daemon shutdown (`kill_all`) now stops the exit-hook watchers still polling — never a hook already running — like n2's detach watchers, so the next daemon's watcher is the one that runs it.
- **Create pane: placement reset on a type switch** (developer, 2026-11-03): switching to a session type without the control clears the sandbox (types other than claude-cli and cursor-cli) and the jailed codebase (types other than claude-cli), so a placement left on from claude or cursor no longer disables the tmux toggle for opencode, muse or warp (`useCreateSessionPlacement`).
- **Contract deviations kept from /green** (for review): the toggle takes `unavailable` instead of `sandbox` (one rule for claude and the harnesses); pane commands target `=<name>:` because a bare session target fails on tmux 3.6a; `pane_state` reads empty output as the session being gone (tmux 3.6a answers an absent session with success and no output).

## TODO

- [x] Record initial discovery (`2026-11-03-tmux-agent-harnesses-in-tmux-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail) — web and cli-sessions run red; lifecycle compile-checked only, red on n1/n2 stubs
- [x] USER REVIEW — acceptance tests — waived by the developer 2026-11-03 (reviewed with the stack at the end)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
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

Scoped to `tddy-cli-sessions`, `tddy-agent-launch`, `tddy-session-lifecycle` (`cargo check --all-targets` and `cargo clippy --all-targets -- -D warnings` clean on the rebased tree). Red, run: `tddy-cli-sessions` 2 unit tests (`tmux.rs`) + 2 in `tests/tmux_exit_hook.rs`, all failing on `unimplemented!`; `CreateSessionRunInTmuxAcceptance.cy.tsx` 7 of 7 failing — the toggle is not rendered (n1's 3 cases are red on n1's stub, the 4 new ones on this node's). Red, **not run** (heavy test link, shared disk): the five `cli_harness_tmux_acceptance` tests — compile-checked; every one starts through n1's launcher, and resumes through n2's re-attach, so they stay red until n1 and n2 are green (this node is wave 3). Not run: the rest of the workspace (CI's).

Surface published: `remain_on_exit: bool` on `start_agent_tmux_session` (n1's function is still a stub, so it has no callers to update — n1's launcher passes `false`, this node's `cli_harness_tmux` passes `on_exit.is_some()`); `tddy-agent-launch::cli_harness_tmux` (`start_harness_in_tmux`, `resume_harness_in_tmux`) wired into the generic harness `launch` and cursor start/resume; `tddy-cli-sessions` `remain_on_exit_args`, `capture_pane_args`, `capture_final_screen`, `exit_hook_capture`, `CliSessionManager::run_exit_hook_on_agent_exit`; web `CreateSessionRunInTmuxToggle` (stub, rendered for cursor/opencode/muse/warp) and `createSessionRequest.ts` sending `runInTmux` for them. The claude-cli toggle is still n1's to render; the green phase should unify the two.

## Green status

Green on the tree rebased onto `feature/tmux-agent/tmux-resume` (9550dbe25). Scoped runs only:

- `tddy-cli-sessions`: `--lib tmux` 20 of 20; the whole package passes, including `tests/tmux_exit_hook.rs` (2 of 2).
- `tddy-session-lifecycle`: `cli_harness_tmux_acceptance` 5 of 5.
- `tddy-agent-launch --lib`: 174 of 175. The failure is `session_acting_identity_tests::a_local_gh_project_yields_the_gh_logins_four_pairs_and_its_github_source`, on the project identity path this change does not touch. Not yet checked against the base.
- `tddy-web`: `bun test src/components/sessions` 440 of 440; Cypress `CreateSessionRunInTmuxAcceptance` 7 of 7, `createSessionRequestParity` 2 of 2, `CreateSessionRequestBuilder` 17 of 17.

Deviations from the contract, for review:

- The tmux targets for `remain-on-exit` and `capture-pane` are `=<name>:`, not `=<name>`. On tmux 3.6a a bare session target fails with "no such window" and "can't find pane". The two pinned argv tests were corrected to match.
- `pane_state` reads an absent session as success with empty output (observed on tmux 3.6a), so empty output means the session is gone.
- A resumed session whose agent exited while no daemon watched it is ended and started afresh. Its exit hook does not run, and the resume logs a warning.
- The exit-hook watcher polls the pane once a second. The exit settlement is registered only when the agent's exit is seen.
- `CreateSessionRunInTmuxToggle` takes `unavailable` where the contract named `sandbox`, so the claude and harness rules are one rule.

Decided by the developer and implemented at /validate-changes (see Decisions): placement reset on a type switch, and harness re-attach / exit-watch parity after a restart. The earlier deviation "a resumed session whose agent exited while no daemon watched it is ended and started afresh, its exit hook not run" still holds for **ResumeSession** — the lazy re-attach (first stream) now runs the hook on such a pane instead.

Environment, not this change:

- `cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available` needs `tddy-sandbox-runner` on `PATH`, which `./install` provides and this worktree does not build.

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
