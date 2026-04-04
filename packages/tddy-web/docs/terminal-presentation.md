# Terminal presentation (daemon mode)

Technical reference for **`terminalPresentation`** helpers and **`ConnectionScreen`** presentation modes in **`packages/tddy-web/src/components/connection/terminalPresentation.ts`** and **`ConnectionScreen`**.

## Model

- **`TerminalPresentation`**: `hidden` | `overlay` | `mini` | `full`.
- **Attach kinds**: **`new`** (Start New Session, Connect, successful deep-link **connectSession**) vs **`reconnect`** (Resume, successful deep-link **resumeSession**).

## Rules

- **`nextPresentationFromAttach`**: A **new** attach yields **`full`** and a terminal route **push** when the prior presentation is not already **`full`**. A **reconnect** attach yields **`overlay`** and **no** route push.
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

- Bun: **`terminalPresentation.test.ts`**, **`appRoutes.test.ts`**, **`ConnectionScreen.test.tsx`** (import contract).
- Cypress component: **`ConnectionScreen.cy.tsx`** (resume omits history push; connect performs push).

See [web-terminal.md](../../../../docs/ft/web/web-terminal.md) for product-facing behavior.
