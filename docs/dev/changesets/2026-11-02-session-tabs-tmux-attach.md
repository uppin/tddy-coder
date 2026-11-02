# 2026-11-02 — A running tmux session as a session tab

**Type:** Feature

`#session-tabs` 3/7 ([#806](https://github.com/uppin/tddy-coder/pull/806)). `ListTmuxSessions` lists
the tmux sessions on the session's host for the caller's OS user, and `StartTerminalSession` with kind
`tmux` and a `tmux_session` attaches a tab to one — `tmux -T clipboard attach-session -t =<name>`,
spawned **as the session's OS user**, the first started tab that is not the daemon user. Stopping the
tab detaches it. Across:

- `tddy-terminal-rpc` — proto (`ListTmuxSessions`, `TmuxSessionInfo`,
  `StartTerminalSessionRequest.tmux_session`, `TerminalSessionInfo.tmux_session`);
  `TerminalKind::Tmux { session }` and `TerminalKind::from_request(kind, tmux_session)`;
  `TerminalLaunch.os_user`; `TerminalDescriptor.tmux_session`; the `TmuxHost` port
  (`TerminalSessionPorts.tmux`) and `NoTmux`.
- `tddy-cli-sessions` — `CliSessionManager::start_tmux_terminal`; `PtyHandle::tmux_session`, kept on
  the terminal's index entry.
- `tddy-session-lifecycle` — `HostTmux` (`svc_terminal_ports/tmux_host.rs`): the listing as the user
  through the kernel's `run_output_as_user`, the attach argv, `TDDY_TMUX_BINARY`; the roster's `Tmux`
  arm refuses an unreachable user (`CannotBecomeUser`) and an unknown name (`NOT_FOUND`) before it
  spawns.
- `tddy-coder` — the participant passes `NoTmux` and refuses kind `tmux` (`FAILED_PRECONDITION`).
- `tddy-web` — `useTmuxSessions`, the tmux section of `NewTerminalMenu`, `tmux: <name>` tab labels,
  `useSessionTerminals.open(kind, tmuxSession?)`.
- `flake.nix` — `pkgs.tmux` in the dev shell, so CI runs the real-tmux acceptance tests.

Tests: 7 daemon acceptance on a private tmux socket (`tmux_tabs_acceptance`), 7 `tmux_host` units, 3
`terminal_kind_unit`, 2 participant refusals (`connection_service_participant`), 4 Cypress
(`SessionTerminalTabsTmuxAcceptance`).

Decisions: run as the session's OS user, on their own tmux server; a per-client `-T clipboard` rather
than editing `set-clipboard` / `terminal-features` on the user's server; `=<name>` for an exact match;
the tab's name checked against the listing before anything spawns, so an unknown name never leaves a
dead tab; `TDDY_TMUX_BINARY` as an env var like `TDDY_CLAUDE_BINARY` (a `DaemonConfig` field has ~150
construction sites); the web keeps a loading state so the menu never flashes "No tmux sessions" while
listing.

Backlog: `2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine` gains the `ListTmuxSessions`
refusal (stays open); `2026-10-24-reattach-does-not-replay-the-alternate-screen` records that tmux
tabs are covered by the snapshot open (stays open for non-web replays);
`2026-09-06-unprivileged-daemon-cannot-probe-another-os-user` stays open; new
`2026-11-02-tmux-listing-and-tab-use-two-privilege-drops`.

Code issues: `tddy-terminal-rpc` `oversized-file-service` 522 → 546 (the `ListTmuxSessions`
handler) — not split, the file is stack-shared with #808 and #810.
