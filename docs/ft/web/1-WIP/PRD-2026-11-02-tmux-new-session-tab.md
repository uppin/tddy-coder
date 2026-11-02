# Start a new tmux session as a session tab - PRD

**Date**: 2026-11-02
**PRD Type**: New Feature
**Stack**: `#session-tabs 7/7` (branch `feature/session-tabs/tmux-new`)

## Affected Features

- **Primary**: [Session terminal tabs](../session-terminal-tabs.md) — "+" → **New tmux session…**

## Summary

The "+" menu's tmux section gains **New tmux session…**. It asks for a name, prefilled with a name
derived from the session (unique against the existing tmux sessions), and opens a tab running a new
tmux session in the session's worktree, on the session's host, as the session's OS user. It is then a
tmux tab like any attached one (`#session-tabs 3/7`):

- it is labelled `tmux: <name>`;
- it is shared through the session's room;
- closing it detaches, so the tmux session outlives the tab and even the tddy session.

## Background

`#session-tabs 3/7` attaches to tmux sessions that already exist. Starting one from tddy gives a
persistent shell that survives the tab, the browser and tddy session stops, and can be picked up from
a real terminal later.

## Proposed Changes

### What's Changing

- `StartTerminalSession` with kind `tmux-new` and a tmux session name runs
  `tmux -T clipboard new-session -A -s <name> -c <worktree>` as the session's OS user.
  - The worktree is resolved the same way as for any started tab (live main terminal, or the session's
    recorded worktree when stopped).
  - `-A` attaches if a session with that name appeared in the meantime, instead of failing.
- An empty name, or one tmux rejects (containing `:` or `.`), is refused with `invalid_argument`.
- **Web:** **New tmux session…** in the tmux section opens an inline name field, prefilled with
  `<session-short-id>` or `<session-short-id>-2`, `-3`, … when taken. Enter starts the tab; Escape
  cancels.

### What's Staying the Same

- Attaching to existing tmux sessions, the close-detaches rule, and bash and harness tabs.

## Impact Analysis

### Technical Impact

- `tddy-terminal-rpc`: one more terminal kind.
- `tddy-session-lifecycle`: the new-session argv on the tmux spawn path from node 3.
- `tddy-web`: the menu item and name field.

### User Impact

- A durable shell next to the agent, in one click.

## Acceptance Criteria

- [ ] Starting a `tmux-new` tab named `work` creates tmux session `work` whose first pane's working
  directory is the session's worktree, and the tab is attached to it.
- [ ] Stopping the tab leaves `work` running, detached.
- [ ] Starting `tmux-new` with a name that already exists attaches to it.
- [ ] An empty name or one containing `:` is refused with `invalid_argument`.
- [ ] The web "+" menu → New tmux session… shows a name field prefilled with a name not in the listed
  tmux sessions. Enter opens a `tmux: <name>` tab.

## Out of Scope

- Killing tmux sessions; tmux layouts or windows; tmux in a jail.
