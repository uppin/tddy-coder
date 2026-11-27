# 2026-11-27 — iOS Simulator capture is desaturated (Display P3) and at the window's displayed size

**Category:** Future enhancement (needs a developer decision on a dependency)
**Source:** `CS-2026-11-26-ios-simulator-screen-sharing` (PR #1060), § Technical Debt

- **Colour.** xcap copies the window image's bytes in the display's colour space (Display P3 on
  Apple displays) with no conversion, and the stream is treated as sRGB, so colours look washed
  out. (This is separate from the RGBA→I420 byte-order fix in `tddy-screenshare`, which landed in
  the same PR.)
- **Sharpness.** The window is captured at its displayed size: Device Hub's default zoom shows a
  1179×2556 device at 594×1292 px. Device Hub's "Physical/pixel-accurate" zoom gives native pixels;
  capturing above the displayed size needs another capture API.
- **Fix options:** `CGWindowListCreateImage` drawn into an sRGB bitmap context (deprecated API,
  needs `#[allow(deprecated)]`), or ScreenCaptureKit (`objc2-screen-capture-kit`, a new
  dependency) which also gives native-resolution capture.
- **Why deferred:** both options need a developer decision (a deprecated API or a new dependency).
