# Changesets Applied

Wrapped changeset history for tddy-web.

- **2026-03-28** [Feature] Fullscreen terminal Terminate (SIGINT) — Optional **Terminate** overlay on **`GhosttyTerminalLiveKit`** when **`connectionOverlay.onSessionTerminate`** is set; **`ConnectionService.SignalSession`** with **`SIGINT`** matches Connection Screen **Interrupt**; **`delegateSignalSessionRpc`** shares the RPC path with **`handleSignalSession`**; **`ConnectionScreen`** persists **`sessionId`** and shows fullscreen **`connection-error`** on RPC failure; Bun and Cypress coverage for overlay, protobuf intercept, and negative visibility. (tddy-web)
