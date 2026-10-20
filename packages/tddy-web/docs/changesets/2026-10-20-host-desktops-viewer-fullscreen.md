# 2026-10-20 — Full-screen remote desktop viewer with keyboard lock

**Type:** Feature

`ScreenSharingOverlay` gains a Full screen / Exit full screen control that fullscreens the viewer element, locks every key through the Keyboard Lock API where the browser has one, says "Esc leaves full screen" where it does not, and leaves full screen (and unlocks) on any exit, including closing the desktop. `browserFullscreen.ts` is generalised (`requestElementFullscreen`, `useElementFullscreen`; the terminal callers renamed) and `keyboardLock.ts` is new. See [Remote desktop input](../remote-desktop-input.md).
