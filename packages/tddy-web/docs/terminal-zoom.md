# Terminal font zoom (implementation)

Technical reference for pitch-in / pitch-out / reset in **tddy-web**. Product behavior and UX live in [web-terminal.md](../../../docs/ft/web/web-terminal.md) (Connected Terminal UX — Font zoom).

## Module layout

| Area | Location |
|------|----------|
| Bounds and stepping | `src/lib/terminalZoom.ts` (`clampTerminalFontSize`, `pitchInFontSize`, `pitchOutFontSize`, `canPitchIn`, `canPitchOut`, defaults **8–32** px, step **1**) |
| Bridge and parsers | `src/lib/terminalZoomBridge.ts` (`TERMINAL_ZOOM_BRIDGE_EVENT`, `TERMINAL_FONT_SIZE_SYNC_EVENT`, `parseTerminalZoomBridgeDetail`, `parseTerminalFontSizeSyncDetail`, `dispatchTerminalZoomBridge`, `dispatchTerminalZoomBridgeOn`, `dispatchTerminalFontSizeSync`, `isTerminalZoomDebugEnabled`) |
| Terminal integration | `src/components/GhosttyTerminal.tsx` (`minFontSize` / `maxFontSize`, bridge listener, **Ctrl/⌘ +/-/0**, two-finger **pinch**, **trackpad** `wheel`+`ctrlKey`, `applyFontSizePx`, `data-terminal-font-size`, imperative `setTerminalFontSize` on ref; `pinchZoomFont` prop) |
| Connection chrome | `src/components/connection/ConnectionTerminalChrome.tsx` (no zoom UI; status dot / fullscreen only) |
| LiveKit shell | `src/components/GhosttyTerminalLiveKit.tsx` (`fontSize` prop → **`GhosttyTerminal`** baseline) |

## Event contract

Bridge actions use `window.dispatchEvent` with **`tddy-terminal-zoom`**. Payloads are validated before handling. Font-size sync uses **`tddy-terminal-font-size-sync`** (emitted when the applied size changes).

Verbose logging is opt-in: **`VITE_TERMINAL_ZOOM_DEBUG=true`** at build time, or **`debugLogging`** on **`GhosttyTerminal`**.

## Pinch (touch)

With two fingers on the terminal container, **span** (distance between touches) is tracked. Spreading fingers past a small threshold applies **pitch-in**; pinching smaller applies **pitch-out**, reusing the same `applyFontSizePx` path as keyboard and bridge dispatch. Single-finger touch still maps to SGR mouse when the TUI enables mouse tracking. Multi-touch **touchend** does not emit a synthetic mouse-up until all fingers leave the surface, so pinch does not leave dangling press/release pairs.

## Pinch (trackpad — macOS / laptop)

There are no `TouchEvent`s for a built-in trackpad. Browsers map **`pinch-to-zoom` on the trackpad to `wheel` events with `ctrlKey === true`** (you do not press the Ctrl key). The handler uses **`reduceTrackpadPinchAccum`** in **`terminalZoom.ts`** and listens in the **capture** phase on the terminal container so it runs **before** the emulator’s own wheel handling (otherwise xterm would consume the gesture for scrolling). Normal wheel scrolling (without `ctrlKey`) still maps to SGR mouse wheel when the TUI enables mouse tracking.

## Resize path

`Terminal.options.fontSize` updates, then **`FitAddon.fit()`** runs. When column/row dimensions change, the existing **`onResize`** path runs and the virtual TUI receives **`\\x1b]resize;{cols};{rows}\\x07`** on the same channel as keyboard input.

## Testing

- **Bun**: `src/lib/terminalZoom.test.ts`, `src/lib/terminalZoomBridge.test.ts`
- **Cypress (component)**: `cypress/component/TerminalZoomAcceptance.cy.tsx`

## Assumptions

The bridge uses global **`window`** listeners. The product targets a **single** embedded terminal surface per relevant view; multiple independent terminals on one page would need a scoped channel or React context if that becomes a requirement.
