# 2026-11-27 — Resolve the iOS Simulator bridge binary (PR #1060)

**Type:** Feature

`screen_sharing.ios_simulator_binary_path` and `resolve_ios_simulator_binary_path`: the configured
path, else a `tddy-ios-emulator` sibling of the running executable, else the bare name on `PATH`
(`tests/ios_simulator_binary_resolution.rs`).
