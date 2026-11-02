# 2026-11-02 — Session tabs: a new tmux session as a tab (#session-tabs 7/7)

**Type:** Feature

`TerminalKind::TmuxNew { session }` (`TMUX_NEW_KIND`, kind `"tmux-new"`) is mapped by
`TerminalKind::from_request`; the daemon roster validates the name with `tmux_session_name`, builds
`tmux_new_session_argv` (`-T clipboard new-session -A -s <name> -c <worktree>`) and spawns it through
`start_tmux_terminal`, as the attach tab is. The coder participant refuses the kind like `tmux`. Web:
`New tmux session…` with an inline name field, `nextTmuxSessionName`, `tmux: <name>` labels.

Tests: `tmux_new_session_acceptance` (real tmux on a private socket; the two refusals pin their
messages, so they are refused by name validation rather than the unknown-harness path), the
`tmux_host` units, `terminal_kind_tmux_new_unit`, `tmuxSessionName.test.ts` and
`SessionTerminalTabsTmuxNewAcceptance.cy.tsx`. The tmux fixtures both tmux suites use live in
`tddy-session-lifecycle/tests/support/mod.rs`.

Backlog: no entry resolved; the privilege-drop entry
(`2026-11-02-tmux-listing-and-tab-use-two-privilege-drops`) stays open and applies to this kind too.
Part of [#810](https://github.com/uppin/tddy-coder/pull/810).
