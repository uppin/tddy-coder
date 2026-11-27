# 2026-11-27 — An iOS Simulator cannot be pinched or given any two-finger gesture

**Category:** Future enhancement
**Source:** `CS-2026-11-26-ios-simulator-screen-sharing` (PR #1060), § Technical Debt

- The viewer forwards **one** pointer (`useScreenSharingInput` reads `changedTouches[0]`; a mouse is
  one contact) and `IosSimulatorClient` turns it into a single `dtuhidd` digitizer contact
  (`packages/tddy-ios-emulator/src/dtuhid.rs`). The DTUHID wire model has a second contact point
  (`pointTwo` in idb's model), unused here, so pinch-to-zoom, two-finger rotate and two-finger
  scroll cannot be made.
- **Fix shape:** a multi-contact event on `ScreenSharingInputService` (or an option-drag
  convention like Simulator.app's own) mapped to two `dtuhidd` contacts; on the browser side,
  forward every touch in `touches`, not only the first.
- **Why deferred:** single-touch tap, drag and swipe covered the PR's acceptance criteria; a
  multi-contact wire event changes the proto for every protocol.
