# Session Terminal Tabs

## Summary

A session's detail pane gains a **tab bar at the top** that lets the user switch between the coding
agent and one or more interactive shell (bash) terminals. Multiple terminals per session are
supported. The first tab is the **Agent** (the coding-agent terminal, reserved id `"main"`) and is
**not closable**; a **`+`** button opens additional bash terminals, each a closable tab. Switching
tabs never tears a terminal down — every terminal of the focused session stays mounted and keeps
streaming in the background (the same keep-alive model the per-session `SessionRuntimeRegistry`
already uses across sessions).

This works for **both** session transports:

- **Local / `connected-grpc` sessions** (claude-cli, cursor-cli, workspace): served by the daemon's
  existing multi-terminal `ConnectionService` RPCs (`StartTerminalSession` / `StopTerminalSession` /
  `ListTerminalSessions`, and `terminal_id`-addressed `StreamTerminalOutput` / `SendTerminalInput`).
- **Remote / `connected-livekit` sessions** (tddy-coder recipe/tool): the Agent tab is the existing
  VirtualTui over `terminal.TerminalService`; bash tabs are served by the coder's own participant,
  which now spawns shell PTYs and answers the same `terminal_id`-addressed `ConnectionService`
  terminal RPCs (see [Session Participant RPC & Metadata](../coder/session-participant-rpc.md) and
  [Terminal Sessions](../daemon/terminal-sessions.md)).

## Background

Until now the web mounted exactly one terminal per session (the reserved `"main"` terminal) and had
no way to open a shell alongside the agent. The daemon already supported multiple terminals per
session for local sessions, but nothing in the web called those RPCs, and coder/LiveKit sessions had
no shell capability at all. This feature closes both gaps and surfaces them as tabs.

## UX

The tab bar renders at the top of the focused session's runtime area (above the terminal canvas),
styled like the existing inspector tab strip (`InspectorTabs`).

- **Agent tab** (`data-testid="sessions-terminal-tab-agent"`): always present, first, selected by
  default, **no close control**. Renders the session's coding-agent terminal — `GrpcSessionTerminal`
  with `terminal_id="main"` for `connected-grpc`, the VirtualTui `SessionLiveKitTerminal` for
  `connected-livekit`.
- **Bash tabs** (`data-testid="sessions-terminal-tab-<terminalId>"`): one per shell terminal, each
  with a close control (`data-testid="sessions-terminal-tab-close-<terminalId>"`). Closing a bash
  tab calls `StopTerminalSession(terminal_id)`, removes the tab, and — if it was active — returns
  focus to the Agent tab.
- **New-terminal button** (`data-testid="sessions-terminal-tab-new"`): calls `StartTerminalSession`,
  appends the returned `terminal_id` as a new bash tab, and focuses it.
- **Keep-alive**: the active tab's terminal is visible; the others are `display:none` but stay
  mounted and subscribed to their output stream, so switching back is instant and background
  terminals keep receiving bytes.

The terminal-control mutex is unchanged and remains **per session** — a single control lease covers
all of a session's terminals (the Agent and every bash tab share it).

## Requirements

1. A connected session shows a terminal tab bar with an **Agent** tab that has no close control.
2. `+` starts a new bash terminal (`StartTerminalSession`), which appears as a new tab and becomes
   active; its terminal opens `StreamTerminalOutput` for the returned `terminal_id`.
3. Multiple bash terminals per session are supported; switching tabs keeps every terminal of the
   session mounted (background terminals keep streaming).
4. Closing a bash tab calls `StopTerminalSession(terminal_id)`, removes the tab, and falls back to
   the Agent tab when the closed tab was active.
5. Keyboard input routes to the **active** tab's `terminal_id` (`SendTerminalInput`).
6. On (re)attach the tab bar reflects the session's live terminals via `ListTerminalSessions`.

## Non-goals

- Renaming terminal tabs.
- Reordering / drag-and-drop of tabs.
- Persisting open bash terminals across daemon (or coder) restart — terminals are in-memory.
- Per-terminal control leases (the mutex stays per-session).

## Related

- [Terminal Sessions (daemon)](../daemon/terminal-sessions.md)
- [Session Participant RPC & Metadata (coder)](../coder/session-participant-rpc.md)
- [Web Terminal](web-terminal.md)
