# 2026-11-27 — Booted iOS Simulators as desktop channels (PR #1060)

**Type:** Feature

`ios_simulators.rs`: `SimulatorLister`, `BridgeBinarySimulatorLister` (`tddy-ios-emulator
--list-devices`), `ios_simulator_channels` (`ios:<udid>`, "<name> · <runtime>", kind
`IOS_SIMULATOR`, port 0, `device_id`). `SubprocessHostToolingProbe` lists them
(`listing_ios_simulators_through` / `_from`) into `HostTooling.ios_simulators`, appended to
`remote_desktop`. `HostDesktopTarget.device` holds a target's udid. A lister error lists none.
`host_tooling.rs` grew 463 → 517 production lines (recorded in
`docs/dev/todo/2026-11-27-host-tooling-rs-over-the-500-line-budget.md`). See
[host-tooling-probe.md](../host-tooling-probe.md#ios-simulator-channels).
