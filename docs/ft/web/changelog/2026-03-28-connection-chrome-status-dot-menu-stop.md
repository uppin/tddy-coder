# 2026-03-28 — Connection chrome: status dot, menu, Stop

- **`GhosttyTerminalLiveKit`** **`connectionOverlay`**: Top-left **build id**; top-right **status dot** with **`data-connection-status`** (values **`connecting`**, **`connected`**, **`error`**); dot menu lists **Disconnect** and **Terminate** when **`onTerminate`** is provided (SIGTERM). **Stop** (`data-testid="terminal-stop-button"`) sits bottom-right and enqueues **0x03** on the same terminal input queue as keyboard interrupt. Implementation: **`ConnectionTerminalChrome`**, **`dataConnectionStatusValue`**; pulse animation for **`connecting`** respects **`prefers-reduced-motion`**; menu dismisses on outside click or **Escape**.
- **ConnectedTerminal** (**standalone** and **ConnectionScreen**): During JWT fetch, the fullscreen container shows the same chrome so the primary loading indicator is the dot (not a **`livekit-status`**-only screen).
- **ConnectionScreen**: Connected state carries **`sessionId`**; **Terminate** in the dot menu invokes **`SignalSession`** (SIGTERM), aligned with the session table **Signal** dropdown.
- **Tests**: Bun **`connectionChromeStatus.test.ts`**; Cypress component specs **`App.cy.tsx`**, **`ConnectionScreen.cy.tsx`**, **`GhosttyTerminalLiveKit.cy.tsx`**.
- **Feature doc**: [web-terminal.md](../web-terminal.md) (Connection chrome; Fullscreen terminal session chrome).
