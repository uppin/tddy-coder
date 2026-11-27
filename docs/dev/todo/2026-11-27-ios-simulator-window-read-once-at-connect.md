# 2026-11-27 — An iOS Simulator window's position and device area are read once, at connect

**Category:** Future enhancement
**Source:** `CS-2026-11-26-ios-simulator-screen-sharing` (PR #1060), § Technical Debt — `TODO(ios-simulator)` in `packages/tddy-ios-emulator/src/macos.rs`

- `XcapSimulatorWindows::find` reads the window bounds and the device area (below the title bar
  in Simulator.app; the `iOSContentGroup` accessibility frame in Device Hub) once, and
  `DtuhidInput` and `DeviceViewport` keep them for the whole stream. A window moved, resized, or
  re-zoomed after Connect is captured at the new size but cropped and mapped with the old
  rectangle; closing and reopening the desktop recovers.
- Shares the size-change half with `2026-10-18-screen-share-mid-session-resize.md` (rotation, and
  any resize that changes the frame size, stops the stream updating).
- **Fix shape:** re-read bounds and device area when a capture's size differs from the expected one
  (or periodically), and republish on a size change once the mid-session-resize item lands.
- **Why deferred:** the PR's scope was a stable window; the size-change path does not exist in the
  streamer yet.
