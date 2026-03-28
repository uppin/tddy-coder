# Fullscreen terminal overlay (LiveKit)

Technical reference for the **`GhosttyTerminalLiveKit`** connection chrome above the Ghostty canvas.

## Controls

| Control | Mechanism | When present |
|--------|-----------|----------------|
| **Build ID** | Top-left label from **`connectionOverlay.buildId`** | When **`buildId`** is defined |
| **Ctrl+C** | **`enqueueTerminalInput(new Uint8Array([0x03]))`** — PTY input path | Always when **`connectionOverlay`** is set |
| **Terminate** | Parent **`onSessionTerminate`** callback (async supported) | Only when **`onSessionTerminate`** is defined |
| **Disconnect** | **`connectionOverlay.onDisconnect`** | Always when **`connectionOverlay`** is set |

Right offsets (from the viewport’s right edge): **Disconnect** at **8px**, **Terminate** at **100px** when shown, **Ctrl+C** at **192px** when **Terminate** is present, otherwise **72px** for **Ctrl+C**.

## Terminate vs Ctrl+C

- **Ctrl+C** injects **ETX (0x03)** through the same batched queue as keyboard and resize sequences (LiveKit **`StreamTerminalIO`** input path).
- **Terminate** relies on the parent to perform daemon signaling. In **`ConnectionScreen`**, the callback is **`handleSignalSession(sessionId, Signal.SIGINT)`**, which uses **`delegateSignalSessionRpc`** to call **`ConnectionService.SignalSession`**. Failures set **`error`** and, for the fullscreen path, **`rethrowOnFailure: true`** so **`GhosttyTerminalLiveKit`** does not mark RPC completion on failure.

## Module: `sessionTerminateOverlay.ts`

- **`buildTerminateOverlayAriaLabel()`** — Accessible name for the **Terminate** button.
- **`handleTerminateOverlayClick(callback, { trace })`** — Awaits the parent callback; **`trace`** follows **`debugLogging`** on **`GhosttyTerminalLiveKit`**.
- **`delegateSignalSessionRpc({ ... })`** — Invokes the injected **`signalSession`** function; **`trace`** follows **`import.meta.env.DEV`** when called from **`ConnectionScreen`**.

## Related feature documentation

- [Web terminal — Connected Terminal UX](../../../../docs/ft/web/web-terminal.md)
