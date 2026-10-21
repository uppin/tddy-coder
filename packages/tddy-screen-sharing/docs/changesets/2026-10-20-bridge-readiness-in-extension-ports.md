# 2026-10-20 — The bridge's start report is read from the extension ports

**Type:** Architecture

`#host-desktops` 6/8 ([#658](https://github.com/uppin/tddy-coder/pull/658)); correction to
`2026-10-20-bridge-ready`. `bridge_readiness.rs`, the inline `FakeBridge` and `tests/common`
import the protocol from `tddy_daemon_extensions::bridge_readiness`, and the `tddy-screenshare`
dependency is dropped: that crate is LiveKit's, and `livekit_is_a_satellite` allows no library
outside the plugin, wiring and known-offender lists to name it. Cross-package record: [2026-10-20-bridge-readiness-in-extension-ports.md](../../../../docs/dev/changesets/2026-10-20-bridge-readiness-in-extension-ports.md).
