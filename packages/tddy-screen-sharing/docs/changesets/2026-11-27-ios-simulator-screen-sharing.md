# 2026-11-27 — iOS Simulator host targets (PR #1060)

**Type:** Feature

`PROTOCOL_IOS_SIMULATOR` routes to `tddy-ios-emulator`. `AddHostTarget` refuses a non-loopback host
or an empty `device_id` with `INVALID_ARGUMENT`; the udid is stored on the target, listed as
`device_id`, and passed as `BridgeSpawnConfig.device` with port 0. A simulator starts with
`DesktopAuthMethod::None` and raises no credential prompt
(`tests/ios_simulator_targets_acceptance.rs`). See
[screen-sharing-service.md](../screen-sharing-service.md#an-ios-simulator-target-is-local-and-signs-in-with-nothing).
