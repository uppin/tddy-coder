# Terminal presentation (daemon mode)

Technical reference for **`terminalPresentation`** helpers and **`ConnectionScreen`** presentation modes in **`packages/tddy-web/src/components/connection/terminalPresentation.ts`** and **`ConnectionScreen`**.

## Model

- **`TerminalPresentation`**: `hidden` | `overlay` | `mini` | `full`.
- **Attach kinds**: **`new`** (Start New Session, Connect, successful deep-link **connectSession**) vs **`reconnect`** (Resume, successful deep-link **resumeSession**).

## Rules

- **`nextPresentationFromAttach`**: A **new** attach yields **`overlay`** and **no** automatic route **push** (`shouldPushTerminalRoute: false`). A **reconnect** attach yields **`overlay`** and **no** route push. **`ConnectionScreen`** applies **`replace`** navigation to `/terminal/{sessionId}` after RPC success so the address bar matches the focused attachment; **Expand** uses **push** when entering **`full`**.
- **`applyOverlayPreviewClickToFull`**: Expands from overlay preview to **`full`** without mutating connection counters (no second **connectSession** / **resumeSession**).
- **`applyDedicatedTerminalBackToMini`**: Fullscreen **Back** yields **`mini`** without incrementing disconnect counters.
- **`reconcileReconnectOverlayInstances`**: Maps any positive reconnect signal count to a single logical overlay instance (show-at-most-once).
- **`defaultTerminalMiniOverlayPlacement`**: Returns **`bottom-right`** for floating overlay/mini placement.

## UI wiring

- **`ConnectedTerminal`** accepts **`terminalLayout`**: **`fullscreen`** | **`overlay`** | **`mini`**. Compact layouts use a fixed **160px** width and default bottom-right positioning; **Expand** and **Back** controls carry **`data-testid`** values **`terminal-reconnect-expand`** and **`terminal-back-to-mini`** where applicable.
- Floating overlay mounts under **`data-testid="terminal-reconnect-overlay-root"`** while the session list remains visible.

## Routing helpers

- **`terminalDeepLinkSessionPath`** in **`appRoutes.ts`** matches **`terminalPathForSessionId`** (including URL-encoded session ids).

## Tests

- Bun: **`terminalPresentation.test.ts`**, **`multiSessionState.test.ts`**, **`appRoutes.test.ts`**, **`ConnectionScreen.test.tsx`** (import contract).
- Cypress component: **`ConnectionScreen.cy.tsx`** (multi-session attach roots, partial disconnect, inactive prune); resume vs connect history behavior.

See [web-terminal.md](../../../../docs/ft/web/web-terminal.md) for product-facing behavior.
