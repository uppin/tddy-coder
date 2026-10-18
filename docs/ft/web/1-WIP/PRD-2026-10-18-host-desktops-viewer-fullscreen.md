# Full-screen remote desktop viewer with keyboard lock — PRD

**Date**: 2026-10-18
**PRD Type**: Feature Enhancement
**Stack**: `#host-desktops` 5/8. Umbrella on `feature/host-desktops/service-split`.

## Affected Features

- [Screen sharing sessions](../screen-sharing-sessions.md): AC-SS-6 (Escape and the chord), AC-SS-11
- [Remote desktop input](../../../../packages/tddy-web/docs/remote-desktop-input.md)

## Summary

The desktop viewer, on both scopes, has a **Full screen** control that hands the viewer to the
browser's Fullscreen API, and an **Exit full screen** control while it is full screen. On entering
full screen, where the browser offers the Keyboard Lock API, the viewer locks the keyboard, so Esc,
Alt+Tab and similar keys reach the desktop. Where it does not, the viewer says that Esc leaves full
screen. `Ctrl+Alt+Esc` still closes the desktop and also leaves full screen. Leaving full screen by
any route (browser Esc, the control, closing) unlocks the keyboard.

## Background

`ScreenSharingOverlay` is a `fixed inset-0` page overlay, so the browser's chrome and the OS stay
visible, and browser-reserved keys never reach the desktop. `src/lib/browserFullscreen.ts` already
wraps the Fullscreen API for the terminal panes.

## Proposed Changes

### What's Changing
- `browserFullscreen.ts` generalised from "connected terminal" naming to any element, with terminal callers updated.
- A `keyboardLock` helper: lock and unlock, with support detection.
- `ScreenSharingOverlay`: a full-screen toggle, a state-aware label, a hint about Esc when the keyboard is not locked, and cleanup on unmount.

### What's Staying the Same
- Input scaling (the picture's rendered rect is still the reference) and the closing chord.

## Success Criteria
- [ ] The control requests full screen on the viewer element, not the page
- [ ] While full screen, the control reads "Exit full screen" and exits it
- [ ] Where Keyboard Lock exists, entering full screen locks the keyboard and leaving unlocks it
- [ ] Where it does not, the viewer says Esc leaves full screen
- [ ] Closing the viewer while full screen leaves full screen

## Out of Scope
- Picture-in-picture, scaling modes (fit/1:1).
