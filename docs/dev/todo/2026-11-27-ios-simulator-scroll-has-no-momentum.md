# 2026-11-27 — A scroll on an iOS Simulator is a short swipe with no momentum

**Category:** Future enhancement
**Source:** `CS-2026-11-26-ios-simulator-screen-sharing` (PR #1060), § Technical Debt

- `IosSimulatorClient::inject_wheel` (`packages/tddy-ios-emulator/src/client.rs`) turns each
  `ScreenSharingWheelEvent` — one per animation frame, summed in the browser — into a complete
  touch-down, 4 moves and lift (`swipe_path`, `SWIPE_STEPS`). Each lift ends the gesture, so iOS
  never sees a fling and a long trackpad scroll moves content in short steps without inertia. A
  scroll is also ignored while a touch is down.
- **Fix shape:** hold one contact down across consecutive wheel events and lift it after a short
  idle (or on the trackpad's momentum phase), so the device sees one continuous drag.
- **Why deferred:** a per-event swipe was enough to make simulator content scrollable from a
  wheel; continuous gestures need timing state in the client and a test clock.
