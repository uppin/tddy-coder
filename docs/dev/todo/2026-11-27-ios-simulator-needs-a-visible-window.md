# 2026-11-27 — An iOS Simulator streams only while its window is visible on the console

**Category:** Future enhancement
**Source:** `CS-2026-11-26-ios-simulator-screen-sharing` (PR #1060), § Technical Debt

- `tddy-ios-emulator` captures the Simulator.app / Device Hub window (xcap), so the device's window
  must be on screen and unminimised on the logged-in console. A headless Mac, a locked screen, or a
  minimised window cannot stream; a closed window fails the start with
  `WindowNotFound`. (Input does not depend on the window: it goes to the device's `dtuhidd`.)
- **Fix shape:** capture the device framebuffer directly — idb's `idb_companion` video stream, or
  SimulatorKit / CoreSimulator IOSurface access — instead of the window.
- **Why deferred:** idb as a dependency is an external tool to install; a direct framebuffer path
  is private-API work of its own. Window capture covered the PR's acceptance criteria.
