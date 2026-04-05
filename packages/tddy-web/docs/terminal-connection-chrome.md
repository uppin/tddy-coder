# Terminal connection chrome (tddy-web)

Technical reference for LiveKit connection UI around **`GhosttyTerminal`**.

## Components

- **`TerminalConnectionStatusBar`**: Top chrome row (`data-testid="terminal-connection-status-bar"`) with **`role="toolbar"`** and **`aria-label="Terminal connection"`**. Wraps **`ConnectionTerminalChrome`** for **`GhosttyTerminalLiveKit`**, **`ConnectionScreen`**, and the standalone connected entry in **`index.tsx`**.
- **`ConnectionTerminalChrome`**: Status dot (menu: Disconnect, optional Terminate), optional build id, optional fullscreen control. Layouts:
  - **`statusBar`** — Full-width toolbar row; primary path for embedded **`GhosttyTerminalLiveKit`**.
  - **`corner`** — Dot and controls positioned over the terminal canvas (legacy overlay positioning).
  - **`paneHeader`** — Compact dot + menu for floating pane chrome; no build id / fullscreen in that branch.
- **`connectionTerminalChromeDotStyles`**: Shared **`CONNECTION_TERMINAL_DOT_STYLES`** for pulse / state styling across layouts.

## GhosttyTerminalLiveKit integration

- **`connectionOverlay`**: When set, the status bar + **`ConnectionTerminalChrome`** (`chromeLayout="statusBar"`) render above the terminal flex column.
- **`connectionChromePlacement`**:
  - **`floating`** (default): Full bar — build id, dot, fullscreen, optional **`statusBarEndSlot`** (e.g. mobile keyboard control).
  - **`none`**: Compact bar — dot + menu and optional end slot; **`statusBarShowBuildId`** and **`statusBarShowFullscreen`** resolve to false so mini / overlay presentations omit build id and fullscreen.
- **`fullscreenTargetRef`**: Selects the element for the Fullscreen API (typically the connected terminal container). Falls back to an internal wrapper when unset.
- **`onConnectionStatusChange`**: Reports **`connecting` | `connected` | `error`** for hosts that hide duplicate LiveKit copy when the floating chrome row is suppressed.

Diagnostic traces use **`tddyDevDebug`** from **`tddyDevLog`** (dev-oriented logging, not a parallel production code path).

## Geometry helpers

**`terminalStatusBarLayout.ts`** (pure functions, no DOM reads in the module):

- **`statusBarBottomMeetsOrAboveTerminalTop`** — Status bar sits at or above the terminal top edge (epsilon-tolerant).
- **`plannedChromeCentersClearTerminalCanvas`** — No control bounding-box center lies inside the terminal rectangle.
- **`controlCenterStrictlyInsideRect`** — Center-point inclusion test for nested rects.

**`terminalStatusBarLayout.test.ts`** (Bun) covers the helpers. Cypress **`GhosttyTerminalLiveKit.cy.tsx`** imports the same functions for layout assertions.

## Related product doc

- [Web terminal — Connected Terminal UX](../../../../docs/ft/web/web-terminal.md) (connection chrome, mobile UX, fullscreen).
