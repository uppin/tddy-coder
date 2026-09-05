# 2026-06-25 — **Terminal mobile shortcut drawer

**Type:** Feature

per-tool key preset buttons** — `ShortcutDrawer` (floating drag-to-snap panel, `position:fixed`, snaps to nearest edge on drop, `data-snap-edge`); `toolShortcuts.ts` (`ToolShortcutDef`, `TOOL_SHORTCUTS`, `keySequenceToBytes`, `resolveShortcutsForSession`); `GhosttyTerminalLiveKit` `mobileShortcuts`/`mobileShortcutsViewportHeight` props; `LiveKitConnectionParams.shortcuts`; 5 `addSessionAttachment` sites in `ConnectionScreen` resolve shortcuts at attach; Cypress `ShortcutDrawer.cy.tsx` + 3 GhosttyTerminalLiveKit integration tests; flaky Disconnect test root-cause fix (stub alias shadowing + `force:true`). Feature [web-terminal.md](../ft/web/web-terminal.md). (tddy-web)
