# Session Terminal Tabs

## Summary

A session's detail pane has a **tab bar at the top** that lets the user switch between the coding
agent and one or more extra terminals. Multiple terminals per session are supported. The first tab is
the **Agent** (the coding-agent terminal, reserved id `"main"`) and is **not closable**; a **`+`**
menu opens additional terminals, each a closable tab. An extra terminal runs the login shell
(**Bash**), a registered CLI harness (Claude CLI, Cursor CLI, OpenCode, Muse Code, Warp) — an
**agent tab**: a second, interactive agent in the same worktree as the session's main agent — or a
**tmux tab**: a client attached to a tmux session already running on the session's host for the
session's OS user. The agents share the checkout and do not talk to each other; the operator
coordinates them. Switching
tabs never tears a terminal down — every terminal of the focused session stays mounted and keeps
streaming in the background (the same keep-alive model the per-session `SessionRuntimeRegistry`
already uses across sessions).

This works for **both** session transports:

- **Host-served sessions** (claude-cli, cursor-cli, workspace) — a session connection carrying
  `{rpc}` only: served by the daemon's multi-terminal `terminal_session.TerminalSessionService`
  RPCs (`StartTerminalSession` / `StopTerminalSession` / `ListTerminalSessions`, and
  `terminal_id`-addressed `StreamTerminalOutput` / `SendTerminalInput`).
- **Room-backed sessions** (tddy-coder recipe/tool) — a connection whose capabilities include
  `media`: the Agent tab is the
  VirtualTui over `terminal.TerminalService`; bash tabs are served by the coder's own participant,
  which spawns shell PTYs and answers the same `terminal_id`-addressed `TerminalSessionService`
  terminal RPCs (see [Session Participant RPC & Metadata](../coder/session-participant-rpc.md) and
  [Terminal Sessions](../daemon/terminal-sessions.md)).

## UX

The tab bar renders at the top of the focused session's runtime area (above the terminal canvas),
styled like the inspector tab strip (`InspectorTabs`).

- **Agent tab** (`data-testid="sessions-terminal-tab-agent"`): always present, first, selected by
  default, **no close control**. Renders the session's coding-agent terminal — one
  `GhosttyTerminalSession` either way. What is chosen from the session connection's capabilities is
  the **feed** behind it: `GrpcSessionTerminal`'s own `TerminalSessionService` stream with
  `terminal_id="main"` for a host-served session, the connection's room feed for a media-capable
  one. See [terminal-session.md](../../../packages/tddy-web/docs/terminal-session.md).
- **Terminal tabs** (`data-testid="sessions-terminal-tab-<terminalId>"`): one per started terminal
  (bash or agent), each with a close control
  (`data-testid="sessions-terminal-tab-close-<terminalId>"`). Closing a tab calls
  `StopTerminalSession(terminal_id)`, removes the tab, and — if it was active — returns focus to the
  Agent tab.
- **Tab labels** are the terminal's kind with a per-kind ordinal, counted in tab order: **Bash 1**,
  **Bash 2**, **Warp 1**, **Claude CLI 1**. The kind name is the harness's label from the Create
  pane's session-type toggle, or **Bash** for the login shell. A tmux tab is labelled by the tmux
  session it is attached to instead — **tmux: alpha**. On reattach the labels are rebuilt from the
  `kind` (and, for tmux, the `tmux_session`) each terminal carries in `ListTerminalSessions`, so a
  reloaded page — and every other viewer — shows the same names.
- **New-terminal menu** (`+`, `data-testid="sessions-terminal-tab-new"`): toggles a menu
  (`data-testid="sessions-terminal-tab-new-menu"`) whose items are **Bash**
  (`sessions-terminal-tab-new-bash`), then one per CLI harness
  (`sessions-terminal-tab-new-<session type>`: `claude-cli`, `cursor-cli`, `opencode-cli`,
  `muse-cli`, `warp-cli`). The harness list is the same one the Create pane's session-type toggle
  reads (`cliHarnessOptions.ts`), so a harness added there appears here too. Choosing an item calls
  `StartTerminalSession` with that `kind`, appends the returned `terminal_id` as a tab, and focuses
  it. **Escape** (which returns focus to `+`) or a click outside closes the menu without starting
  anything.
  - On a **tool** (room-backed, tddy-coder) session the menu offers **Bash** only — its terminals are
    served by the coder's participant, which runs no harness registry.
  - **Before the session's type is known** (the session is not yet listed in the drawer) the menu
    offers every harness. The host refuses a kind it does not run, and the refusal is shown in
    `sessions-terminal-error` with no tab opened — the same surface as any refused start.
  - The `+` is the **last item of the tab list**, right after the last tab of any kind (terminal,
    child session, agent conversation). When the tabs overflow and scroll it is `sticky` at the
    scroller's right edge with an opaque background, so it stays reachable. Its menu opens under the
    button, outside the scroller, so the scroller does not clip it, and stays inside the viewport
    even when `+` sits at the viewport's right edge.
  - **tmux section**: below the bash and harness group, a group of its own listing the host's tmux
    sessions (`ListTmuxSessions`, fetched afresh **each time the menu opens** — tmux sessions come and
    go outside tddy). Each item (`sessions-terminal-tab-new-tmux-<name>`) shows the session's name,
    its window count and, when any, its attached clients. Choosing one calls `StartTerminalSession`
    with kind `tmux` and that `tmux_session`, and opens a **tmux: <name>** tab attached to it. While
    the listing is in flight the section says "Loading tmux sessions…"
    (`sessions-terminal-tab-new-tmux-loading`) rather than flashing an empty host; an empty listing
    shows **No tmux sessions** (`sessions-terminal-tab-new-tmux-empty`); a refused listing (tmux not
    installed) shows its reason inline (`sessions-terminal-tab-new-tmux-error`). The bash and harness
    items are unaffected by whatever state the tmux listing is in. A tool session's menu has no tmux
    section.
  - **Closing a tmux tab detaches** its client: the tmux session keeps running and can be attached
    again from the menu or from any real terminal. tmux runs on the alternate screen, which a
    reattaching browser's snapshot open carries, so the tmux screen renders correctly on reload.
- **Keep-alive**: the active tab's terminal is visible; the others are `display:none` but stay
  mounted and subscribed to their output stream, so switching back is instant and background
  terminals keep receiving bytes.

The terminal-control mutex is unchanged and remains **per session** — a single control lease covers
all of a session's terminals (the Agent and every bash or agent tab share it).

An agent tab is a terminal tab in every other respect too: focus, keep-mounted, full screen and
per-terminal input routing behave as for bash. It starts the harness fresh and interactive (no model,
no first prompt, no resume) and, like a bash tab, gets no session environment and is not persisted —
see [Terminal Sessions](../daemon/terminal-sessions.md#agent-terminals).

### Full screen

The tab strip carries a trailing **⛶ full-screen control**
(`data-testid="sessions-terminal-fullscreen"`), pinned to the right edge outside the tabs' own
horizontal scroller so a session with many terminals cannot push it out of reach. It puts the
**active pane** into browser full screen via the Fullscreen API — the same
`lib/browserFullscreen.ts` helper the standalone connect screen's terminal already uses.

- **The active pane** is whichever tab holds the pane — the Agent terminal, a bash or agent terminal, a
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

### Stopped sessions

A session whose agent is not running (`is_active == false`) keeps its tab strip. Its worktree is
still on disk, so the operator can run the tests, read git state or ask another agent about it
without resuming the agent first.

- **Agent tab**: what the agent terminal last printed, from `GetTerminalHistory` for `main`, in a
  read-only terminal drawn in the **history colour scheme** (the greyed scheme of "View history"). It
  scrolls through the whole recording, takes no input, shows no "Claim terminal" toast and takes no
  Files-tab insert. A session with no recording says **No recorded output**.
  - A **toast** over it reads "This session is not running" with a **Resume** action. While the
    resume runs the action shows a spinner and cannot be pressed again; a refused resume shows its
    reason in the toast, and the greyed view stays.
  - The recorded ACP transcript is the inspector's **Activity** tab — see
    [Inactive session activities](inactive-session-activities.md#inspector-activity-tab).
- **Terminal tabs**: the terminals the daemon already runs for the session (`ListTerminalSessions`),
  and **`+`** opens Bash or any CLI harness. They stream through the owning daemon's
  `TerminalSessionService` exactly as a live session's tabs do, and start in the worktree the
  session's record names (see [Terminal Sessions](../daemon/terminal-sessions.md#start--stop)). A
  session with no worktree on this host — a split session's agent half, or an unknown id — is
  refused, and the refusal shows in `sessions-terminal-error`.
- **Agent-conversation tabs**: **Add agent** opens a conversation tab in the strip, as on a live
  session.
- **The terminal tabs carry the live strip's features**: full screen, the mobile shortcut drawer on
  the focused tab, the Files tab's **Insert** into the focused bash tab, and "Claim terminal" with
  the grey-out while another screen holds the session's control lease.
- **Resume** (the toast's or the top bar's — one action) swaps the stopped strip for the live one
  as soon as `ResumeSession` succeeds, without waiting for the next session-list poll. Tabs opened
  while the session was stopped are daemon terminals, so the live strip lists them; open
  conversations are not cancelled. Leaving
  the session and coming back does not cancel them either: only closing a tab ends a conversation.
- A workflow session (PR-Stack, workflow chat) keeps its own view and has no strip.

## Requirements

1. A connected session shows a terminal tab bar with an **Agent** tab that has no close control.
2. `+` opens a menu of **Bash** plus every CLI harness. Choosing an item starts a terminal of that
   kind (`StartTerminalSession` with `kind`), which appears as a new tab and becomes active; its
   terminal opens `StreamTerminalOutput` for the returned `terminal_id`. Escape or a click outside
   closes the menu without starting anything.
3. Multiple terminals per session are supported; switching tabs keeps every terminal of the
   session mounted (background terminals keep streaming).
4. Closing a terminal tab calls `StopTerminalSession(terminal_id)`, removes the tab, and falls back to
   the Agent tab when the closed tab was active.
5. Keyboard input routes to the **active** tab's `terminal_id` (`SendTerminalInput`).
6. On (re)attach the tab bar reflects the session's live terminals via `ListTerminalSessions`, with
   the same kind-aware labels (**Bash 1**, **Warp 1**, …).
7. The tab bar carries a full-screen control that puts the **active** pane into browser full screen,
   flips to an "exit" affordance while active, and leaves every terminal of the session mounted
   across the transition.
8. A tool session's menu offers **Bash** only; a session whose type is not yet known offers every
   harness.
9. A refused start shows its reason in `sessions-terminal-error` and opens no tab.
10. A stopped (non-workflow) session shows the strip too: an **Agent** tab holding the agent
    terminal's last recorded output, greyed and read-only, under a "This session is not running"
    toast with **Resume**; the terminals the daemon lists for it; its agent conversations; and `+`.
    Its terminals start in the session's recorded worktree and are still in the strip, the same
    terminals, after Resume. Resume goes live at once, not at the next session-list poll.
11. The menu lists the host's tmux sessions for the session's OS user when it opens; choosing one
    opens a **tmux: <name>** tab attached to it, which every viewer lists under the same label.
    Closing it detaches, leaving the tmux session running. The section shows **No tmux sessions**
    for an empty listing and the refusal's reason when tmux is unavailable.

## Non-goals

- Renaming terminal tabs.
- Reordering / drag-and-drop of tabs.
- Tiling two terminals side by side — the panes are a tabbed deck, not a split.
- Restoring full screen from a URL, or a keyboard shortcut for it.
- Persisting open terminals (bash or agent) across a daemon (or coder) restart — terminals are
  in-memory. They do survive the session itself stopping and resuming, since the daemon holds them.
- A model, first prompt or provider account for an agent tab; agent tabs inside a sandboxed
  session's jail; agent-to-agent messaging.
- Per-terminal control leases (the mutex stays per-session).
- Creating or killing a tmux session from tddy; tmux inside a sandboxed session's jail; tmux on a host
  other than the session's.

## Related

- [Terminal Sessions (daemon)](../daemon/terminal-sessions.md)
- [Session Participant RPC & Metadata (coder)](../coder/session-participant-rpc.md)
- [Web Terminal](web-terminal.md)
