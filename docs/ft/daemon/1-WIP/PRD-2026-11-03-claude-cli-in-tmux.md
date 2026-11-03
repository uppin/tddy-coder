# A Claude CLI session can run its agent inside tmux - PRD

**Date**: 2026-11-03
**PRD Type**: Feature Enhancement
**Stack**: `#tmux-agent 1/3` (branch `feature/tmux-agent/claude-in-tmux`)

## Affected Features

- **Primary**: [Claude CLI session](../claude-cli-session.md) — a "Run the agent in tmux" option at creation
- **Related**: [Terminal sessions § tmux terminals](../terminal-sessions.md) — the agent's tmux session is an
  ordinary tmux session on the host, attachable from any terminal
- **Related**: [Session drawer / Create pane](../../web/session-drawer.md) — the new checkbox

## Summary

When a **claude-cli** session is created with **Run the agent in tmux** (a new Create-pane checkbox, host
sessions only), the daemon starts Claude inside a detached tmux session on the session's host, **as the
session's OS user**, rooted in the session's worktree. It then attaches the session's main terminal to that
tmux session. The tmux session's name is recorded in the session's `.session.yaml`, so the choice is
persistent.

- The session's liveness, **Signal** and **Delete** act on the agent itself (tmux's pane process), not on
  the tmux client the main terminal runs.
- **Delete** also kills the tmux session.
- The agent runs with the same environment it would get outside tmux: the toolcall socket, hooks, provider
  route and renderer settings are passed to the tmux pane, not only to the client.

This node keeps today's lifecycle around the option:
- terminating the session or stopping the daemon ends the tmux session;
- **Resume** starts a fresh tmux session running `claude --resume`.

Re-attaching to a live tmux session is `#tmux-agent 2/3`; the other harnesses are `3/3`.

## Background

A tmux-run agent keeps its terminal in a place the operator already uses. They can attach to it from their
own terminal over SSH, and `#tmux-agent 2/3` builds on it to survive daemon restarts. The `#session-tabs`
stack added tmux tabs (attach and new session) with an as-the-session-user tmux path. Those helpers live
privately in `tddy-session-lifecycle`, which the launcher (`tddy-agent-launch`) cannot depend on, so they
move down to `tddy-cli-sessions` here.

## Proposed Changes

### What's Changing

- **Create request:** `StartSessionRequest.run_in_tmux` (bool). It is honoured for host claude-cli
  sessions. It is refused with `invalid_argument` together with `sandbox`, `sandboxed_codebase` or a split placement (`codebase_daemon_instance_id`), and ignored by other session
  types until `3/3`.
- **Record:** `SessionMetadata.tmux_session: Option<String>`. The name is `tddy-<full session id>`.
- **Launch:**
  - `tmux new-session -d -s <name> -c <worktree> -e <K=V>… -- <claude argv>` runs as the session's OS user;
  - then the main terminal runs `tmux -T clipboard attach-session -t =<name>` as that user;
  - the recorded `pid` is tmux's `#{pane_pid}` (the Claude process).
- **Signal / Delete:** Signal signals the pane pid (unchanged code path, correct pid). Delete kills the pane
  pid and runs `tmux kill-session -t =<name>`.
- **Daemon shutdown:** a tmux-run main terminal's tmux session is killed with the others.
  `#tmux-agent 2/3` changes this to detach.
- **Resume:** any leftover tmux session of that name is killed, then a fresh one runs Claude's resume argv.
- **Helpers moved:** tmux binary resolution, argv builders, session-name validation, listing and
  reachability move from `tddy-session-lifecycle` to `tddy-cli-sessions` (the tmux tab code uses them from
  there, unchanged).
- **Web:** a "Run the agent in tmux" checkbox (`create-session-tmux-toggle`) for claude-cli. It is disabled
  while the session is sandboxed.

### What's Staying the Same

- Sessions created without the option.
- Sandboxed sessions.
- tmux tabs.
- Claude's hooks (file-based) and the control lease.

## Impact Analysis

### Technical Impact

- `tddy-service` proto; `tddy-changeset` (`SessionMetadata`); `tddy-cli-sessions` (a `tmux` module and a
  main-terminal-in-tmux start); `tddy-agent-launch` (claude start/resume, delete); `tddy-session-lifecycle`
  (tmux tab code now imports the moved helpers; daemon shutdown); `tddy-web` (Create pane).
- `SessionMetadata` is `deny_unknown_fields`: a daemon older than this change cannot read a tmux-run
  session's record. That is accepted and documented (downgrades are not supported).

### User Impact

- A Claude session whose agent lives in tmux: visible in `tmux ls` on the host, and attachable from any
  terminal.

## Acceptance Criteria

- [ ] Creating a claude-cli session with `run_in_tmux` starts a tmux session named in `.session.yaml`, whose
  pane runs the Claude binary in the worktree, as the session's OS user.
- [ ] The session's main terminal is attached to it: input reaches the pane, and output streams back.
- [ ] The recorded `pid` is the pane's process, and the session reads active while it runs.
- [ ] Claude in the pane sees the session's environment (e.g. `TDDY_SOCKET`).
- [ ] Delete kills the agent and the tmux session.
- [ ] Resume of a terminated tmux-run session starts a fresh tmux session running Claude's resume argv.
- [ ] `run_in_tmux` together with `sandbox` is refused with `invalid_argument`.
- [ ] The Create pane offers the checkbox for claude-cli, and sends `run_in_tmux`.
- [ ] tmux tabs behave as before (their suites stay green).

## Out of Scope

- Re-attaching to a live tmux session and surviving a daemon restart (`2/3`).
- Other harnesses (`3/3`).
- tmux inside a jail.
