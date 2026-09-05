# Terminal connection chrome (tddy-web)

Technical reference for the connection UI around **`GhosttyTerminal`**. The terminal it wraps is
**`GhosttyTerminalSession`** ([terminal-session.md](terminal-session.md)), which renders every session
whichever way its bytes travel, so this chrome is the same on all of them.

## Components

- **`TerminalConnectionStatusBar`**: Top chrome row (`data-testid="terminal-connection-status-bar"`) with **`role="toolbar"`** and **`aria-label="Terminal connection"`**. Wraps **`ConnectionTerminalChrome`** for **`GhosttyTerminalSession`**, **`ConnectionScreen`**, and the standalone connected entry in **`index.tsx`**.
- **`ConnectionTerminalChrome`**: Status dot (menu: Disconnect, optional Terminate), optional build id, optional fullscreen control. Layouts:
  - **`statusBar`** — Full-width toolbar row; primary path for embedded **`GhosttyTerminalSession`**.
  - **`corner`** — Dot and controls positioned over the terminal canvas (legacy overlay positioning).
  - **`paneHeader`** — Compact dot + menu for floating pane chrome; no build id / fullscreen in that branch.
- **`connectionTerminalChromeDotStyles`**: Shared **`CONNECTION_TERMINAL_DOT_STYLES`** for pulse / state styling across layouts.

## `GhosttyTerminalSession` integration

- **`connectionOverlay`**: When set, the status bar + **`ConnectionTerminalChrome`** (`chromeLayout="statusBar"`) render above the terminal flex column.
- **`connectionChromePlacement`**:
  - **`floating`** (default): Full bar — build id, dot, fullscreen, optional **`statusBarEndSlot`** (e.g. mobile keyboard control).
  - **`none`**: Compact bar — dot + menu and optional end slot; **`statusBarShowBuildId`** and **`statusBarShowFullscreen`** resolve to false so mini / overlay presentations omit build id and fullscreen.
- **`fullscreenTargetRef`**: Selects the element for the Fullscreen API (typically the connected terminal container). Falls back to an internal wrapper when unset.
- **`connectionStatus`**: **`connecting` | `connected` | `error`**, supplied by the caller that owns the connection. Drives the status dot and the raw **`livekit-status`** readout; a caller whose connection state is shown elsewhere omits it, and the readout then stays out of the layout.

Diagnostic traces use **`tddyDevDebug`** from **`tddyDevLog`** (dev-oriented logging, not a parallel production code path).

## Geometry helpers

**`terminalStatusBarLayout.ts`** (pure functions, no DOM reads in the module):

- **`statusBarBottomMeetsOrAboveTerminalTop`** — Status bar sits at or above the terminal top edge (epsilon-tolerant).
- **`plannedChromeCentersClearTerminalCanvas`** — No control bounding-box center lies inside the terminal rectangle.
- **`controlCenterStrictlyInsideRect`** — Center-point inclusion test for nested rects.

**`terminalStatusBarLayout.test.ts`** (Bun) covers the helpers. Cypress **`GhosttyTerminalSessionChrome.cy.tsx`** imports the same functions for layout assertions.

## Related product doc

- [Web terminal — Connected Terminal UX](../../../../docs/ft/web/web-terminal.md) (connection chrome, mobile UX, fullscreen).
