# Attach a running tmux session as a session tab - PRD

**Date**: 2026-11-02
**PRD Type**: New Feature
**Stack**: `#session-tabs 3/7` (branch `feature/session-tabs/tmux-attach`)

## Affected Features

- **Primary**: [Session terminal tabs](../session-terminal-tabs.md) — "+" lists the host's tmux
  sessions; choosing one opens a tab attached to it
- **Related**: [Web terminal](../web-terminal.md) — a tmux tab is an ordinary terminal tab, so it is
  shared with everyone viewing the session (LiveKit-served sessions included)

## Summary

The "+" menu gains a **tmux** section listing the tmux sessions running on the session's host for the
session's OS user (the user's own tmux server). Choosing one opens a tab, labelled `tmux: <name>`,
running a tmux client attached to that session. Closing the tab **detaches**: the tmux session keeps
running and can be attached again from tddy or from any real terminal.

## Background

Operators already keep long-running work in tmux on their hosts. Attaching to it from the session
view puts that terminal next to the agent and shares it through the session's room, which a plain
`ssh + tmux attach` cannot do.

## Proposed Changes

### What's Changing

- **RPC:** `TerminalSessionService.ListTmuxSessions` returns the host's tmux sessions for the
  requesting OS user: name, window count, attached-client count.
  - No tmux server running answers an empty list, not an error.
  - tmux not installed answers `failed_precondition` naming tmux.
- **RPC:** `StartTerminalSession` with kind `tmux` and a tmux session name starts a tab running
  `tmux -T clipboard attach-session -t <name>` **as the session's OS user**, so it reaches that user's
  tmux server.
  - `-T clipboard` tells tmux this client accepts OSC 52, so tmux copy-mode copies reach the browser
    (`#session-tabs 4/7` turns them into clipboard writes). The user's tmux configuration is not
    modified.
  - An unknown session name is refused with `not_found`.
- **Listing:** `ListTerminalSessions` reports a tmux tab with kind `tmux` and its tmux session name, so
  every viewer labels it `tmux: <name>`.
- **Close:** `StopTerminalSession` ends the tab's tmux client only, which detaches it.
- **Web:** the "+" menu shows a **tmux** section listing the sessions when opened. It shows "No tmux
  sessions" when the list is empty and an inline error when tmux is unavailable.
- tool sessions (the `tddy-coder` participant roster) refuse kind `tmux`, as they refuse harness kinds.

### What's Staying the Same

- Bash and harness tabs, their identity (daemon user) and their menu entries.
- The tmux server and its options.

## Impact Analysis

### Technical Impact

- `tddy-terminal-rpc` proto and service: one RPC, a start field and a list field.
- `tddy-session-lifecycle` / `tddy-cli-sessions`: list tmux sessions and spawn the attach client as the
  mapped OS user through the same spawn path the main agent PTY uses.
- `tddy-web`: the tmux section of `NewTerminalMenu`, the tab label.

### User Impact

- Any tmux session on the host becomes a shareable session tab in two clicks.

## Acceptance Criteria

- [ ] With a tmux server running sessions `alpha` and `beta` for the OS user, `ListTmuxSessions` returns
  both with their window and attached counts.
- [ ] With no tmux server, `ListTmuxSessions` returns an empty list.
- [ ] Starting a `tmux` tab for `alpha` attaches a client to it: input typed into the tab reaches the
  tmux pane, and its output streams back.
- [ ] Stopping that tab leaves the tmux session `alpha` running with zero attached clients.
- [ ] Starting a `tmux` tab for an unknown session name is refused with `not_found`.
- [ ] `ListTerminalSessions` reports the tab as kind `tmux` with the name `alpha`.
- [ ] The web "+" menu lists `alpha` and `beta`; choosing `alpha` opens a tab labelled `tmux: alpha`.
- [ ] The menu shows "No tmux sessions" for an empty list.

## Out of Scope

- Creating a tmux session (`#session-tabs 7/7`).
- tmux inside a session's jail.
- Killing a tmux session from tddy.
- tmux on another host than the session's.
