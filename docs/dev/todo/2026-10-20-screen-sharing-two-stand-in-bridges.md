# 2026-10-20 — tddy-screen-sharing has two stand-in bridges

**Category:** Test debt
**Source:** `#host-desktops` 6/8 (`feature/host-desktops/bridge-ready`, PR #658), `/validate-tests`

- `packages/tddy-screen-sharing/src/screen_sharing_service.rs` inline `mod tests` has `FakeBridge`: a
  `/bin/sh` stand-in that records its argv and stdin and prints a `ready` line at 1920×1080.
- `packages/tddy-screen-sharing/tests/common/mod.rs` has `StandInBridge`: a `/bin/sh` stand-in that
  records its pid and does what `TheBridge` scripts (report a readiness line, stay silent, or exit).
  It sits beside minimal copies of the inline suite's operator, credential store and daemon builder.
- Both write the readiness line through the real `BridgeReadiness` serialisation, so a protocol
  change is caught by both suites, but a change to how the daemon spawns a bridge has to be made
  twice.
- **Why deferred:** merging them is not mechanical. An integration-test crate cannot reach the
  library's `#[cfg(test)]` module, so one stand-in needs a `testing` module behind a feature, or
  the inline suite has to move to `tests/`. Either way that is a ~1,300-line move out of
  `screen_sharing_service.rs`, which `#host-desktops` 7/8 (#659) and 8/8 (#660) also touch.
- **Fix shape:** after the stack lands, move the inline suite onto `tests/common/` and give
  `StandInBridge` the argv and stdin recording `FakeBridge` has.
