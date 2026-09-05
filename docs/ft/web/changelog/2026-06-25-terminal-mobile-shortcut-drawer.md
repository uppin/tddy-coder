# 2026-06-25 — Terminal mobile shortcut drawer

- `ShortcutDrawer` component: floating drag-to-snap panel (`position: fixed`; snaps to nearest screen edge on drag release; `data-snap-edge` attribute); renders one button per preset; each click sends the correct ANSI/VT byte sequence via `pushInput`
- `toolShortcuts.ts`: `ToolShortcutDef` interface, `TOOL_SHORTCUTS` map (tddy-coder: Shift+Tab/Ctrl+C/Escape; claude-cli: Escape/Ctrl+R/Ctrl+C), `keySequenceToBytes` (named keys, Ctrl+letter, F1–F12, single chars), `toolIdentifierFromPath`, `resolveShortcutsForSession`
- `GhosttyTerminalLiveKit`: new `mobileShortcuts?: ToolShortcutDef[]` and `mobileShortcutsViewportHeight?: number` props; drawer renders when `showMobileKeyboard && mobileShortcuts.length > 0`
- `LiveKitConnectionParams`: `shortcuts?: ToolShortcutDef[]` field; `resolveShortcutsForSession` called at all 5 `addSessionAttachment` sites in `ConnectionScreen`
- Cypress component tests: `ShortcutDrawer.cy.tsx` (render, empty, click-to-send, drag-snap, row layout); `GhosttyTerminalLiveKit.cy.tsx` 3 integration tests; fixed 2 pre-existing flaky Disconnect tests (stub alias shadowing + `{ force: true }` for canvas `mousedown` interception)
- Feature: [web-terminal.md](../web-terminal.md)
