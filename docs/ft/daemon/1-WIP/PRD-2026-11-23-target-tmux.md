# PRD: tmux on Coder/SSH targets

**Date**: 2026-11-23
**Status**: 🚧 In Progress — n1 wrapped (#1019: [terminal-sessions.md § tmux terminals](../terminal-sessions.md#tmux-terminals)); this PRD wraps with n2, which owns requirements 7–12
**Stack**: `#target-tmux` — n1 (agent run in tmux on the target, #1019), n2 (tmux tabs on the target)

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

Decisions (developer, 2026-11-23): a target session's **+** menu lists the **target's** tmux only;
"<agent> in tmux" runs in tmux **on the target**; a restored tab whose tmux session is gone is
**recreated**, never dropped.

7. **Listing.** `ListTmuxSessions` for a target session lists the tmux sessions of the target login
   (through its login shell, as the caller's subject resolves the target). No tmux server is an empty
   list; a target whose login shell finds no `tmux` is `FAILED_PRECONDITION` naming tmux, shown in the
   menu as a host without tmux is; an unreachable target is `UNAVAILABLE`. The host's tmux is never
   listed for a target session.
8. **Attach / new tmux session tabs.** On a target session the menu opens them with
   `wrappers: [target]`. Both run `tmux new-session -A -s <name>` through the target's login shell in
   the session's worktree there, as a local tab terminal; attach first checks the target lists
   `<name>` (`NOT_FOUND` otherwise). Recorded `[target, tmux-attach{session}]`.
9. **Agent in tmux tabs.** "<agent> in tmux" on a target session opens with `wrappers: [target, tmux]`:
   the agent (found by its bare command on the target) runs inside
   `tmux new-session -A -s tddy-<session>-<terminal>` on the target. Recorded
   `[target, tmux{session}, agent{harness, conversation}]`. Closing the tab kills that tmux session on
   the target, as closing a host agent-in-tmux tab kills its host tmux session.
10. **Restore.** `ResumeSession` restores `[target, tmux-attach]` and `[target, tmux, agent]` tabs by
    running the same `new-session -A` command: re-attached when the tmux session lives, recreated
    when it is gone (the agent resuming its conversation when its harness can). An unreachable
    target fails that tab with the reason; the resume goes on.
11. **Daemon restart.** The startup re-attach never looks for a target tab's tmux on the daemon host;
    it leaves target tabs for `ResumeSession` (it has no caller whose credentials reach the target).
12. Host tmux paths — the close-tab kill and the startup re-attach — never act on a `target`-wrapped
    chain.

## Out of scope

- Automatic reconnect of a dropped target client.
- tmux for sandboxed / split placements (still refused).

## Affected features

- [docs/ft/daemon/terminal-sessions.md § tmux terminals](../terminal-sessions.md#tmux-terminals)
- [docs/ft/web/session-terminal-tabs.md](../../web/session-terminal-tabs.md)
