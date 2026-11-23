# 2026-11-23 — tmux tabs of a target session run on the target (`#target-tmux` 2/2, #1020)

**Type:** Feature · **Packages:** tddy-terminal-rpc, tddy-agent-launch, tddy-session-lifecycle, tddy-session-web, tddy-coder, tddy-testing-commons

## What changed

- `TerminalKind::{TargetTmux, TargetTmuxNew, TargetTmuxAgent}` from `wrappers: [target]` /
  `[target, tmux]`; `TerminalKind` moved to `service/ports/terminal_kind.rs` (ports.rs 437 → 503 →
  239 + 272 production lines).
- `TmuxHost::list_for_session(TmuxListing)` (default: `list`), called by `ListTmuxSessions`;
  `SessionTmux` replaces `HostTmux` and lists a target session's target.
- `TerminalRoster::stop_as(session, terminal, subject)` (default: `stop`): closing a
  `[target, tmux, agent]` tab kills its tmux session on the target; without a caller it is logged and
  left.
- `remote_agent_launch::target_tmux_tab`: `list_target_tmux_sessions`, `target_tmux_tab_command`,
  `launch_tmux_tab_on_target`, `end_target_tmux_session_named`, `target_tmux_session_lives`;
  `TargetAgentTab::tmux_session` wraps an agent tab in `new-session -A`.
- `TabRestore::{TargetTmux, TargetTmuxAgent}`; the restore's `live_tabs` and the startup re-attach skip
  `target` chains.
- Web: on a target session the menu's tmux items carry `[target]`, agent-in-tmux `[target, tmux]`.
- The stand-in target login shell runs `/bin/sh` when started with no arguments (as tmux does).

## Decisions (developer)

The menu lists the target's tmux only; agent-in-tmux runs on the target; a gone tmux session is
recreated, never dropped. Daemon restart restores on the next Resume (no caller at startup).

## Verification

`remote_target_session_acceptance` 27/27 (10 new); web `NewTerminalMenuTargetTmuxAcceptance` 2/2 and
the neighbouring tmux / target menu specs; `./test -p` terminal-rpc, agent-launch, testing-commons;
session-lifecycle lib and 8 tab suites; clippy clean.

## Not covered by a daemon test

A target without tmux: a stand-in target that lacks tmux cannot be built portably (CI's `/usr/bin`
has it); `list_target_tmux_sessions`'s unit test covers the refusal.
