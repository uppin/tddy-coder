# iOS Simulator as a screen-sharing target

**Date**: 2026-11-26
**PRD Type**: Feature Enhancement
**Changeset**: [CS-2026-11-26-ios-simulator-screen-sharing.md](../../../dev/1-WIP/CS-2026-11-26-ios-simulator-screen-sharing.md)

## Affected Features

- **Primary**: [Screen sharing sessions](../screen-sharing-sessions.md), the host scope: a new kind of desktop
- [Hosts screen](../hosts-screen.md), the remote-desktop section of a macOS host row
- [Remote desktop input](../../../../packages/tddy-web/docs/remote-desktop-input.md), reused unchanged

## Summary

On a Mac that runs a tddy daemon, every **booted iOS Simulator** shows up as a desktop on that
host's row, for example "iPhone 16 Pro · iOS 18.2". Connecting streams the **device screen only**
(no window chrome). A click or drag in the viewer becomes a tap or swipe on the device, and typing
enters text. An agent on the same Mac installs and launches builds with `xcrun simctl`, and the
result appears in the shared screen. Emulators get one crate per OS: `tddy-ios-emulator` now,
Android later.

## Acceptance Criteria

- **AC-IOS-1**: A macOS host lists exactly its booted simulators as desktops. Shut-down and unavailable devices are not listed. A Mac without Xcode, or a non-macOS host, lists none and raises no error.
- **AC-IOS-2**: Starting a simulator desktop streams a frame whose size is the device screen in pixels, not the window.
- **AC-IOS-3**: A press, drag and release in the viewer arrives at the device as touch-down, move and touch-up at the matching device points.
- **AC-IOS-4**: Printable keys type their character. Return, Backspace, Escape, Tab and the arrow keys act as those keys.
- **AC-IOS-5**: Starting a device that is shut down or unknown fails with a message naming the device (udid and state). There is no silent black stream.
- **AC-IOS-6**: Without the Screen Recording or Accessibility permission, start fails and names the missing permission.
- **AC-IOS-7**: A simulator desktop can only be bridged on the Mac that runs it. A target naming another host is refused.

## Out of Scope

- Android (`tddy-android-emulator`, a later change)
- Headless or locked-screen Macs (needs `idb`), rotation mid-stream, scroll wheel, pinch
- Whether a sandboxed agent may run `simctl` (developer, 2026-11-26: ignored for now)
