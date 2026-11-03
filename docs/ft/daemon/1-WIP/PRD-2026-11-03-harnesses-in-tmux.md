# Every CLI harness session can run its agent in tmux - PRD

**Date**: 2026-11-03
**PRD Type**: Feature Enhancement
**Stack**: `#tmux-agent 3/3` (branch `feature/tmux-agent/harnesses-in-tmux`)

## Affected Features

- **Primary**: [Cursor CLI](../cursor-cli-session.md), [OpenCode CLI](../opencode-cli-session.md),
  [Muse CLI](../muse-cli-session.md), [Warp CLI](../warp-cli-session.md) sessions — "Run the agent in tmux"
- **Related**: [Claude CLI session](../claude-cli-session.md) — the same option, from `1/3`

## Summary

"Run the agent in tmux" extends from claude-cli to **cursor-cli, opencode-cli, muse-cli and warp-cli**
host sessions, with the start, terminate, resume and re-attach behaviour of `#tmux-agent 1/3` and `2/3`.

- In tmux mode **every harness runs as the session's OS user** (cursor-cli normally runs as the daemon
  user; outside tmux that stays unchanged).
- **Warp's resume token**, read from the terminal when Warp exits, still works. The tmux session keeps
  its last screen (`remain-on-exit`) until the daemon has read the token from it, then it is killed.

## Background

`1/3` and `2/3` built the mechanism on claude-cli. The other harnesses differ only in argv, resume argv and
exit hooks.

## Proposed Changes

### What's Changing

- **Start:**
  - cursor-cli (`start_cursor`, with its chat id) and the generic harness launcher (opencode, muse, warp)
    start inside tmux when `run_in_tmux` is set, as the session's OS user, with the tmux name recorded.
  - `run_in_tmux` stops being ignored for them.
- **Resume:** the per-harness resume paths (cursor `--resume <chat>`, opencode/muse `--continue`,
  warp `--resume <token>`) use `2/3`'s re-attach-or-fresh branch.
- **Exit hooks:**
  - a tmux-run harness's exit hook runs when its **agent** exits (the tmux pane is dead), not when the
    client exits;
  - its output is the pane's final screen (`tmux capture-pane` of a `remain-on-exit` pane), from which
    Warp's token is parsed;
  - the tmux session is killed afterwards.
- **Web:** the checkbox is offered for every CLI harness session type (not tool or workspace sessions),
  and the request stops clearing it for opencode, muse and warp.

### What's Staying the Same

- Non-tmux sessions of every harness, including cursor's daemon-user identity outside tmux.
- Sandboxed sessions.

## Impact Analysis

- `tddy-agent-launch` (cursor and generic start/resume), `tddy-cli-sessions` (exit-hook source for
  tmux-run mains), `tddy-agent-backend` (none expected — argv builders are reused), `tddy-web` (Create pane
  and request).

## Acceptance Criteria

- [ ] Each of cursor-cli, opencode-cli, muse-cli and warp-cli starts in tmux with `run_in_tmux`, as the
  session's OS user, with the name recorded and the pane pid as `pid`.
- [ ] Each resumes with its own resume argv in a fresh tmux session, and re-attaches when the tmux session
  is alive.
- [ ] A Warp session run in tmux records its resume token from the final screen when Warp exits, and the
  next resume passes `--resume <token>`.
- [ ] A client exit does not run a harness's exit hook. An agent exit does.
- [ ] The Create pane offers the checkbox for every CLI harness type and sends `run_in_tmux`.

## Out of Scope

- Tool (tddy-coder) and workspace sessions.
- tmux in a jail.
