# Changeset: Terminal Mobile UX

## Summary

Implements three mobile UX fixes for the web terminal (Android Chrome):

1. **Terminal resize on keyboard open/close** — Uses Visual Viewport API; container height tracks `visualViewport.height`.
2. **Manual keyboard button** — Replaces auto-focus on mobile; floating "Keyboard" button at bottom center; hides when keyboard open.
3. **Touch event forwarding** — Adds touchstart/touchend listeners for tap-to-click in TUI apps.

## Affected Packages

- `packages/tddy-web` — GhosttyTerminal, GhosttyTerminalLiveKit, ConnectedTerminal, new useVisualViewport hook

## Implementation

- **useVisualViewport** (`src/hooks/useVisualViewport.ts`): Subscribes to `window.visualViewport` resize/scroll; returns `{ height, offsetTop, isKeyboardOpen }`.
- **GhosttyTerminal**: Added touchstart/touchend listeners; forwards SGR mouse sequences (same as mouse).
- **GhosttyTerminalLiveKit**: `autoFocus` prop (default true); `onRegisterFocus` callback for keyboard button.
- **ConnectedTerminal**: Uses useVisualViewport for container height; mobile detection via `ontouchstart` or viewport width < 768; keyboard button when `isMobile && !isKeyboardOpen`.

## Tests

- GhosttyTerminal.cy.tsx: Touch event forwarding (touchstart/touchend → SGR sequence).
- useVisualViewport.cy.tsx: Hook returns height and isKeyboardOpen.
- App.cy.tsx: Mobile keyboard button when connected (viewport 375x667).

## Docs

- `docs/ft/web/web-terminal.md`: Added Mobile UX subsection.
