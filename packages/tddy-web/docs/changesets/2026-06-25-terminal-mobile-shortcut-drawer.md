# 2026-06-25 — **Terminal mobile shortcut drawer

**Type:** Feature

per-tool key preset buttons** — `ShortcutDrawer` floating drag-to-snap panel; `toolShortcuts.ts` (`ToolShortcutDef`, `TOOL_SHORTCUTS`, `keySequenceToBytes`, `resolveShortcutsForSession`); `GhosttyTerminalLiveKit` `mobileShortcuts`/`mobileShortcutsViewportHeight` props; `LiveKitConnectionParams.shortcuts` field; 5 `addSessionAttachment` call sites in `ConnectionScreen` resolve shortcuts at attach time; Cypress `ShortcutDrawer.cy.tsx` + 3 integration tests; flaky Disconnect test root-cause fix (stub alias shadowing). Feature [web-terminal.md](../../../../../docs/ft/web/web-terminal.md). (tddy-web)
