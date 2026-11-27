# 2026-11-27 — Desktop's own host borrows media; wheel and touch input docs (PR #1060)

**Type:** Feature

The IPC local-host connection stays `{"rpc"}`, and is lent the common room's `media` when that room
reaches the same daemon (`DesktopIpcHostAcceptance.cy.tsx`). Remote-desktop input forwards wheel and
touch (`ScreenSharingInputForwardingAcceptance.cy.tsx`). See [local-host-ipc.md](../local-host-ipc.md)
and [remote-desktop-input.md](../remote-desktop-input.md).
