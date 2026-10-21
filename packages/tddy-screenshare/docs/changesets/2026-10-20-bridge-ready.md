# 2026-10-20 — Bridges report how their start went

**Type:** Feature

`#host-desktops` 6/8 ([#658](https://github.com/uppin/tddy-coder/pull/658)). `readiness.rs` is new: `BridgeReadiness`, `FailureKind`,
`write_readiness`/`read_readiness` and the `AuthenticationRefused` marker error. `run_bridge` is
split into `start_bridge` and the pump loop, and writes exactly one readiness line to stdout:
`Ready` with the framebuffer size, or `Failed` with a kind classified from the error chain.
`AuthenticationRefused` implements `Display`/`Error` by hand, because the crate has no
`thiserror`. See [Bridge start report](../bridge-readiness.md).
