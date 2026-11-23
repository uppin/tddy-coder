# Changeset: tmux tabs of a target session run on the target (`#target-tmux` 2/2)

**Date**: 2026-11-23
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — four crates and the web package (tddy-terminal-rpc, tddy-agent-launch,
tddy-session-lifecycle, tddy-session-web), a spawned-process seam (real tmux on a stand-in target),
start / list / close / restore / daemon-restart paths

PRD: [PRD-2026-11-23-target-tmux.md](../../ft/daemon/1-WIP/PRD-2026-11-23-target-tmux.md) (n2:
requirements 7–12) · Node 1: [2026-11-23-target-tmux.md](2026-11-23-target-tmux.md) (#1019)

## Responsibility

On a target session: the **+** menu's tmux section lists the target's tmux; attach / new tmux session
tabs and "<agent> in tmux" tabs run in tmux on the target; closing an agent-in-tmux tab kills its
tmux session there; resume re-attaches or recreates them. Host tmux paths never act on a target
chain.

## Boundaries

- **In:** `TerminalKind` (target tmux kinds), the session-aware tmux listing, the roster's target tmux
  tab starts and close, the restore plan + carrying it out, the startup re-attach skip, the web menu's
  wrappers on a target session.
- **Out:** automatic reconnect (TODO from n1); killing tab tmux sessions on delete (host does not
  either — unchanged).

## Dependencies

n1 (#1019): `target_login_run`, `TmuxNotOnTarget`, `probe_tmux_on_target`, the stand-in target's
private `TMUX_TMPDIR`. Never re-implemented here.

## Decisions & Trade-offs (developer, 2026-11-23)

| Fork | Decision |
|---|---|
| What the + menu lists on a target session | **The target's tmux only.** `ListTmuxSessions` decides by the session's record; the web adds `wrappers: [target]` to attach / new. |
| Agent in tmux on a target | **In scope**: `wrappers: [target, tmux]`, chain `[target, tmux, agent]`. |
| A restored tab whose tmux session is gone | **Recreated** (`new-session -A`), never dropped. So the target restore needs no liveness snapshot: one command re-attaches or recreates. |
| Daemon restart | The startup re-attach **leaves** target tabs for `ResumeSession`: it runs with no caller, and a target's credentials resolve only for a caller's subject. Challenge noted: "daemon restart" in requirement 7 is met by the next resume, not eagerly. |

One command runs every target tmux tab, through the target login shell (as n1's main):

```text
cd '<worktree>' && exec "${SHELL:-/bin/sh}" -lc 'exec tmux new-session -A -s '\''<name>'\'' [<login exec of the agent>]'
```

## State A

- `TerminalKind::from_wrapped_request` refuses anything but the login shell or an agent inside
  `target` → tmux tabs on a target session are host tabs (the host's tmux, in the session dir).
- `HostTmux::list(os_user)` lists the host's tmux whatever the session.
- Restore: `target` around anything but `shell`/`agent` is dropped; `live_tabs` checks every `tmux`
  node against the **host's** tmux.
- `reattach_live_tmux_tabs` re-attaches every roster `tmux` node on the host's tmux.
- Close: `end` kills any `tmux` node's session on the **host**.

## State B — draft PR contract

`packages/tddy-terminal-rpc/src/service/ports.rs`:

- `TerminalKind::TargetTmux { session }` — kind `tmux` + `wrappers: [target]`.
- `TerminalKind::TargetTmuxNew { session }` — kind `tmux-new` + `wrappers: [target]`.
- `TerminalKind::TargetTmuxAgent { agent }` — agent kind + `wrappers: [target, tmux]`.
- `TmuxHost::list_for_session(&self, listing: &TmuxListing<'_>)` with a default delegating to
  `list(os_user)`; `pub struct TmuxListing<'a> { os_user, subject, session_id }`. `ListTmuxSessions`
  calls it.

`packages/tddy-agent-launch/src/remote_agent_launch/target_tmux_tab.rs`:

- `pub async fn list_target_tmux_sessions(shell, worktree) -> Result<Vec<TmuxSession>, RemoteLaunchError>`
- `pub fn target_tmux_tab_command(worktree, name, inner: Option<&str>) -> String`
- `pub async fn launch_tmux_tab_on_target(manager, session_id, terminal_id, tab: TargetTab, name) -> Result<Arc<PtyHandle>, RemoteLaunchError>`
- `TargetAgentTab::tmux_session: Option<String>` — `launch_agent_tab_on_target` wraps the agent in
  `new-session -A -s <name>` when set (no new parameter: `launch_agent_tab_on_target` stays at 5).
- `pub async fn end_target_tmux_session_named(shell, name)`; n1's `end_target_tmux_session` delegates.

`packages/tddy-agent-launch/src/opencode_cli_spawn/restore_tabs/plan.rs`:

- `TabRestore::TargetTmux { session }` for `[target, tmux-attach]`.
- `TabRestore::TargetTmuxAgent { session, harness, resume_conversation }` for `[target, tmux, agent]`.

`packages/tddy-session-lifecycle`: `SessionTmux` (the daemon's `TmuxHost`: target sessions list the
target's tmux through `TabTargetShell`); the roster starts the three kinds and records
`[target, tmux-attach]` / `[target, tmux, agent]`; close kills a `[target, tmux, …]` tab's tmux session on
the target (`TerminalRoster::stop` gains the caller's subject); `reattach_live_tmux_tabs` leaves
target chains.

`packages/tddy-session-web/src/NewTerminalMenu.tsx`: on a target session, tmux attach / new open with
`[target]`, "<agent> in tmux" with `[target, tmux]`.

## Acceptance tests

`packages/tddy-session-lifecycle/tests/remote_target_tmux_tabs/mod.rs` (a module of
`remote_target_session_acceptance`: the real daemon, the stand-in ssh target with real tmux on its
private `TMUX_TMPDIR`):

- `a_target_sessions_tmux_listing_lists_the_targets_tmux_sessions`
- `a_new_tmux_session_tab_on_the_target_runs_that_tmux_session_there_and_is_recorded_on_the_target`
- `attaching_a_tab_to_a_tmux_session_on_the_target_runs_a_client_of_it_there`
- `attaching_a_tab_to_a_tmux_session_the_target_lacks_is_refused_not_found`
- `an_agent_in_tmux_tab_on_the_target_runs_its_agent_in_a_tmux_session_of_its_own_there`
- `closing_an_agent_in_tmux_tab_on_the_target_kills_its_tmux_session_there`
- `a_resume_re_attaches_a_target_tmux_tab_whose_tmux_session_lives`
- `a_resume_recreates_a_target_tmux_tab_whose_tmux_session_is_gone`
- `a_resume_resumes_the_agent_of_an_agent_in_tmux_tab_whose_tmux_session_is_gone_on_the_target`
- `the_startup_re_attach_leaves_target_tmux_tabs_for_resume`

`packages/tddy-session-web/cypress/component/NewTerminalMenuTargetTmux.cy.tsx`:

- `a target session's tmux items open on the target`
- `a target session's agent in tmux opens in tmux on the target`

## Unit tests

- `tddy-terminal-rpc` ports: the three wrapped kinds; `[target, tmux]` around a shell is refused.
- `restore_tabs_tests.rs`: `[target, tmux-attach]` → `TargetTmux`; `[target, tmux, agent]` →
  `TargetTmuxAgent` resuming / fresh; missing session → dropped naming it.
- `target_tmux_tab` tests: the command (stand-in tmux), the listing parse / no-server / no-tmux.

## Prerequisites (Step 2b)

- ⚠ During — `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md`:
  new code goes in `svc_terminal_ports/target_tmux_tabs.rs`, never `connection_service.rs`.
- ⚠ During — `packages/tddy-agent-launch/docs/code-issues/complexity-remote-agent-launch-launch-on-target.md`:
  the tmux name goes on `TargetAgentTab`, never a new parameter.
- — [2026-11-02-tmux-listing-and-tab-use-two-privilege-drops.md](../todo/2026-11-02-tmux-listing-and-tab-use-two-privilege-drops.md):
  host-only; a target listing drops no privilege.
- — [2026-11-23-target-tmux-auto-reconnect.md](../todo/2026-11-23-target-tmux-auto-reconnect.md): out of scope.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Acceptance tests (failing) — 10 daemon tests fail at the target refusal / host listing; 2 web
  tests fail on the missing `target` wrapper. A target without tmux is covered by the listing's unit
  test (a stand-in target with no tmux on its login `PATH` cannot be built portably: CI's `/usr/bin`
  holds tmux)
- [x] Unit tests (failing) — guards already passing by construction:
  `a_tmux_attach_on_the_target_without_a_session_is_refused`, `a_tmux_on_a_target_wrapping_no_agent_is_dropped`
- [x] Implementation
- [x] TDD green — `remote_target_session_acceptance` 27/27 (10 new); new web spec 2/2 plus the
  neighbouring tmux / target menu specs; `./test -p` terminal-rpc, agent-launch, testing-commons all green (48 targets); session-lifecycle lib + 8 tab suites green; clippy clean on the five touched crates

## Implementation notes

- `TerminalRoster::stop_as(session, terminal, subject)` (default: `stop`): closing a
  `[target, tmux, agent]` tab needs the caller's credentials to reach the target. A close with no
  caller logs and leaves the target tmux session.
- `SessionTmux` replaces `HostTmux`: host listing as before, a target session lists the target.
- Target restores run `has-session` first only to report the outcome (restored vs fresh).
- `live_tabs` and the startup re-attach skip `target` chains: this host's tmux is never asked.
- The stand-in target login shell (testing-commons) runs `/bin/sh` when started with no arguments,
  as tmux starts a new session's shell.
