# 2026-11-24 — Session close: shared test support to fold, and two coverage gaps

**Category:** Test debt (deferred at the wrap of `feature/session-close-and-remove`, PR #1032)
**Source:** the change's red phase and validation (`docs/dev/changesets/2026-11-24-session-close-and-remove.md`)

## Coverage gaps

- **A local conversation in a close.** The lifecycle suites cover a *remote* conversation only
  (`test_util::open_a_remote_conversation`): a local one needs a `SubagentSession`, which only
  `tddy-daemon-rpc`'s harness opens. Add a close case to `session_agent_conversation_acceptance.rs`,
  including an interrupted turn that does not stop (`ClosedTurnLoop::turn_still_runs` must fail the
  TERMINATE step and skip every removal).
- **A closed target session's tab tmux on delete.** `DeleteSession` of a closed target session skips
  each tab's tmux on the target (`SessionTab::without_its_target_tmux`) as well as the main's; only the
  main's is covered (`close_target_session.rs`). A target tmux *tab* case needs the
  `remote_target_tmux_tabs` harness.

## Duplicated test support

- `close_target_session.rs` re-builds the stand-in-ssh daemon of `remote_target_session_acceptance.rs`
  (`ADaemonWithATarget`, the stub agent, the start request): one shared `target_session_support`
  module for both suites.
- A `git(dir, args)` helper is written out in `tmux_agent_support/close_session.rs`, `worktree_loss.rs`,
  `worktree_sync_acceptance.rs` and `target_session_support`: one in `tddy-testing-commons`.
- `worktree_loss.rs`'s `ASessionWorktree` repeats `worktree_sync_acceptance.rs`'s `a_repo_world` (bare
  origin + tracking clone): share the builder.
- `close_events_support` (the `CloseEvents` reader and `UnreadableUntilDropped`) is reached from
  `tmux_agent_support` by a `#[path]` module so the target suite can declare it alone; fold it into a
  shared support module if more suites need it.
- Web: `closeSessionPage.revealEndedRow` copies the More-fold logic of
  `sessionsDrawerPage.selectEndedSession`; one shared "open the fold this row is under" helper.
- Web: `aSessionServiceBackend`'s builders still take `Partial<…Init>` overrides, which `tsc` rejects
  (pre-existing errors at `aSessionEntry` / `aProjectEntry`); `closeSessionBackend.ts` avoids the
  pattern, and the rest of the file should follow it.

## What would close it

Each bullet is independent: the two coverage cases as tests in the named suites, the support folds as
move-only refactors of the test trees.
