# 2026-03-29 — Connection chrome: immersive status, fullscreen, Terminate confirm

- **`GhosttyTerminalLiveKit`** with **`connectionOverlay`**: The **`livekit-status`** text strip stays out of layout for **`connecting`** / **`connected`**; **`data-connection-status`** on the dot carries phase; errors use **`livekit-error`**. Policy helper: **`shouldShowVisibleLiveKitStatusStrip`** (`packages/tddy-web/src/lib/liveKitStatusPresentation.ts`).
- **`ConnectionTerminalChrome`**: Top-right **`terminal-fullscreen-button`** toggles document fullscreen on the terminal target via **`browserFullscreen`** (standard API + prefixed enter/exit). **`confirmRemoteSessionTermination`** wraps **`window.confirm`** before **`onTerminate`** (shared copy for remote session termination).
- **Tests**: Bun specs for **`browserFullscreen`**, **`liveKitStatusPresentation`**, **`remoteTerminateConfirm`**; Cypress component coverage for chrome placement, fullscreen stub, and terminate flows; e2e contracts assert overlay chrome without a visible **`livekit-status`** row during normal connection.
- **Feature doc**: [web-terminal.md](../web-terminal.md) (Connection chrome; Fullscreen terminal session chrome).
