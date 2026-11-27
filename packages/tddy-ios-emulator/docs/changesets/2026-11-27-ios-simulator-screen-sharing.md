# 2026-11-27 — New crate: the iOS Simulator screen-sharing bridge (PR #1060)

**Type:** Feature

`tddy-ios-emulator` streams a booted iOS Simulator's device screen into LiveKit and forwards viewer
input to it, as a `ScreenSharingClient` run by `run_bridge`. Ports `Simctl`, `SimulatorWindows`,
`InputSink`; macOS adapters `XcrunSimctl` (the system `/usr/bin/xcrun`, toolchain env removed),
`XcapSimulatorWindows` (window owners `Simulator` and `Device Hub`; crop below the 28-point title
bar, or to Device Hub's `iOSContentGroup` accessibility element) and `DtuhidInput` (touch, keys and
scroll-as-swipe sent to the device's `dtuhidd` over XPC, ported from meta/idb, MIT —
`THIRD_PARTY_NOTICES`). Modes: bridge, `--inspect` (no sign-in), `--list-devices` (booted devices
as JSON). Screen Recording and Accessibility are preflighted and a missing one is named.

See [architecture.md](../architecture.md). The `#[ignore]`d `simulator_production.rs` compiles and
was not run (needs a booted simulator).
