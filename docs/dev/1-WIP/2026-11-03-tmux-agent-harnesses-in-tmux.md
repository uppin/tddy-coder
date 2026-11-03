# Changeset: Every CLI harness session can run its agent in tmux

**Created:** 2026-11-03
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — four harnesses' start/resume, cursor's identity switch in tmux mode, exit-hook source for tmux-run mains (remain-on-exit + capture-pane for Warp's token), web request/pane changes
**Stack:** `#tmux-agent 3/3` — branch `feature/tmux-agent/harnesses-in-tmux`, base `feature/tmux-agent/tmux-resume`
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
- [ ] `each_cli_harness_starts_in_tmux_as_the_session_os_user`
- [ ] `each_cli_harness_resumes_in_a_fresh_tmux_session_with_its_resume_argv`
- [ ] `a_live_tmux_run_harness_session_is_reattached_on_resume`
- [ ] `a_warp_session_in_tmux_records_its_resume_token_from_the_final_screen`
- [ ] `a_client_exit_does_not_run_a_harness_exit_hook`

**`tddy-web`**: `packages/tddy-web/cypress/component/CreateSessionRunInTmuxAcceptance.cy.tsx` (extended)
- [ ] `the Create pane offers "Run the agent in tmux" for every CLI harness type`
- [ ] `opencode, muse and warp send run_in_tmux when checked`

### Unit / integration tests
- [ ] `tddy-cli-sessions`: `start_agent_tmux_session` with `remain_on_exit` sets the option; exit-hook source reads `capture-pane` on agent exit only.

## Decisions & Trade-offs

- **`remain-on-exit` + `capture-pane`** for the exit hook — the pane's final screen is the only place Warp's banner survives once the pane's process exits.
- **cursor as the session's OS user in tmux mode only** (developer, 2026-11-03).

## TODO

- [x] Record initial discovery (`2026-11-03-tmux-agent-harnesses-in-tmux-initial-discovery.md`)
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
