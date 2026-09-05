# 2026-03-21 — Mobile keyboard: Ctrl+letter sends control bytes

- **GhosttyTerminalLiveKit**: `handleMobileKeyDown` maps **Ctrl+A–Z** to bytes 0x01–0x1A (e.g. **Ctrl+C → 0x03**). Previously only `onInput` ran, so **Ctrl+C** appeared as the letter **`c`** (0x63).
- **Connection overlays**: **Ctrl+C** / **Disconnect** / **build id** render **inside** `GhosttyTerminalLiveKit` (`connectionOverlay` prop) **above** the terminal (`z-index: 100`, DOM after canvas) and call the same **`enqueueTerminalInput`** queue as keyboard — fixes clicks hitting the canvas and only logging `'c'`/`'v'` from Ghostty `onData`.
