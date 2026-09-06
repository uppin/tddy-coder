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

- **Host-served sessions** (claude-cli, cursor-cli, workspace) — a session connection carrying
  `{rpc}` only: served by the daemon's existing multi-terminal `ConnectionService` RPCs
  (`StartTerminalSession` / `StopTerminalSession` / `ListTerminalSessions`, and
  `terminal_id`-addressed `StreamTerminalOutput` / `SendTerminalInput`).
- **Room-backed sessions** (tddy-coder recipe/tool) — a connection whose capabilities include
  `media`: the Agent tab is the existing
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
  default, **no close control**. Renders the session's coding-agent terminal — one
  `GhosttyTerminalSession` either way. What is chosen from the session connection's capabilities is
  the **feed** behind it: `GrpcSessionTerminal`'s own `ConnectionService` stream with
  `terminal_id="main"` for a host-served session, the connection's room feed for a media-capable
  one. See [terminal-session.md](../../../packages/tddy-web/docs/terminal-session.md).
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

### Full screen

The tab strip carries a trailing **⛶ full-screen control**
(`data-testid="sessions-terminal-fullscreen"`), pinned to the right edge outside the tabs' own
horizontal scroller so a session with many terminals cannot push it out of reach. It puts the
**active pane** into browser full screen via the Fullscreen API — the same
`lib/browserFullscreen.ts` helper the standalone connect screen's terminal already uses.

- **The active pane** is whichever tab holds the pane — the Agent terminal, a bash terminal, a
  spawned child conversation, or a conversation with an attached agent.
- **What goes full screen is the pane stack**, not an individual pane
  (`data-testid="sessions-terminal-pane-stack-<sessionId>"`). Only one pane is ever visible, so the
  operator sees exactly the active terminal — and the stack's siblings, the terminal-control mutex
  overlay and the LiveKit connection overlay, come with it. Handing a single pane to the API instead
  would leave the "Claim terminal" CTA behind the fullscreen layer, so a session whose control
  another screen holds would look interactive while swallowing every keystroke.
- **The tab strip is deliberately left behind** — full screen is the whole viewport for one
  terminal. Because that takes the strip's own toggle with it, the pane draws a floating
  **exit control** (`data-testid="sessions-terminal-fullscreen-exit"`) while it holds fullscreen;
  <kbd>Esc</kbd> works too.
- **Full screen is a view mode.** Nothing unmounts across the transition, so every terminal of the
  session keeps its stream and the session keeps its control lease; the grid re-fits itself through
  the terminal's own `ResizeObserver` / `FitAddon`.
- **The state is not in the URL.** Unlike the inspector's `?full=1` and the Code pane's `?code=1`,
  browser fullscreen cannot be restored from a link — the Fullscreen API requires a user gesture, so
  a shared URL that claimed to reproduce it would silently not.

Known limitation: <kbd>Esc</kbd> exits full screen before the terminal sees it, so a full-screen
`vim` cannot be left with <kbd>Esc</kbd> alone. Lifting that needs the Keyboard Lock API
(`navigator.keyboard.lock(["Escape"])`), which changes the exit gesture to press-and-hold; not done
here.

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
7. The tab bar carries a full-screen control that puts the **active** pane into browser full screen,
   flips to an "exit" affordance while active, and leaves every terminal of the session mounted
   across the transition.

## Non-goals

- Renaming terminal tabs.
- Reordering / drag-and-drop of tabs.
- Tiling two terminals side by side — the panes are a tabbed deck, not a split.
- Restoring full screen from a URL, or a keyboard shortcut for it.
- Persisting open bash terminals across daemon (or coder) restart — terminals are in-memory.
- Per-terminal control leases (the mutex stays per-session).

## Related

- [Terminal Sessions (daemon)](../daemon/terminal-sessions.md)
- [Session Participant RPC & Metadata (coder)](../coder/session-participant-rpc.md)
- [Web Terminal](web-terminal.md)
