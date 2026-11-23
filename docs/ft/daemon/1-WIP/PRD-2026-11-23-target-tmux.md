# PRD: tmux on Coder/SSH targets

**Date**: 2026-11-23
**Status**: 🚧 In Progress
**Stack**: `#target-tmux` — n1 (agent run in tmux on the target), n2 (tmux tabs on the target)

## Summary

`run_in_tmux` on a `StartSessionRequest` that names a `target` (Coder workspace or ssh host) is a
silent no-op today: the agent runs straight under `coder ssh` / `ssh`, and a dropped connection kills
it. After this change the agent runs in a tmux session **on the target**, so it survives a dropped
connection, and a session's tmux tabs (attach / new tmux session) list and run the **target's** tmux
rather than the daemon host's.

## Background

- Local `run_in_tmux` (`#tmux-agent`) runs the agent in tmux session `tddy-<session>` on the daemon
  host; resume re-attaches; delete kills the tmux session and ends its pane's agent.
- A target session's agent runs via `remote_agent_command` over the remote-shell port with no tmux.
  The command records the agent's pid on the target (`.tddy/agents/<id>.pid`) for stop and the
  session pointer's owner.
- The tmux tabs (`docs/ft/web/changelog/2026-11-02-tmux-*.md`) use `HostTmux`, which lists the
  daemon host's tmux even when the session's agent and worktree are on a target.

## Requirements

### n1 — the agent in tmux on the target

1. **Preflight.** A target start or resume with `run_in_tmux` asks the target's login shell whether
   it finds `tmux` (the agent probe of `#tab-hardening` 1/4, for `tmux`), before any worktree is cut.
   A target without tmux refuses with `FAILED_PRECONDITION` naming tmux. **No fallback** to a
   non-tmux run.
2. **Start.** The remote command runs the agent inside `tmux new-session -A -s tddy-<session>` in the
   session's worktree on the target (the same login-shell `env -i` exec as today), and the local PTY
   is the transport client attached to it. The session records `tmux_session: tddy-<session>`.
3. **Identity is the tmux session name, not a pid.** A tmux-run target agent's process is the tmux
   server's child, not something tddy controls, so no pid file is written for it. Liveness during the
   startup window is `tmux has-session -t tddy-<session>` on the target; the session pointer's owner
   pid is read from tmux (`#{pane_pid}`) when it is recorded.
4. **Dropped connection.** The local client exits; the session lists inactive while the agent keeps
   running in tmux on the target. No automatic reconnect (recorded as a TODO).
5. **Resume** runs the same `new-session -A` command: it re-attaches when the tmux session is alive,
   and starts the agent afresh (with its resume arguments) when it is gone.
6. **End.** Delete kills `tddy-<session>` on the target (`tmux kill-session`) through the remote
   shell; a tmux session already gone is not an error, an unreachable target refuses the delete with
   the session intact. Delete never touches the daemon host's tmux for a target session. **Stop**
   (SignalSession) signals the local client and so detaches, leaving the agent running — as Stop of
   a host tmux run signals its attach client.
6a. A daemon restart never looks for a target session's tmux in a tmux server of the daemon host.

### n2 — tmux tabs on the target

7. Restored tabs (`ResumeSession` / daemon restart) that are tmux-run on a target re-attach to
   their tmux session on the target. (n1 re-attaches the **main** on resume; tab restore of
   tmux-run tabs on targets is n2.)
8. On a target session, the **+** menu lists the target login's tmux sessions, and attach / new tmux
   session tabs run `tmux attach` / `tmux new-session` on the target over the remote shell. A target
   without tmux shows the reason, as a host without tmux does.

## Out of scope

- Automatic reconnect of a dropped target client.
- tmux for sandboxed / split placements (still refused).

## Affected features

- [docs/ft/daemon/terminal-sessions.md § tmux terminals](../terminal-sessions.md#tmux-terminals)
- [docs/ft/web/session-terminal-tabs.md](../../web/session-terminal-tabs.md)
