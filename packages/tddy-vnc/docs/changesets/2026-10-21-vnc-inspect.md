# 2026-10-21 — VNC inspect reads the security and VeNCrypt subtype lists

**Type:** Feature

`#host-desktops` 8/8 ([#660](https://github.com/uppin/tddy-coder/pull/660)). `security::inspect` reports the advertised types in the server's order,
with VeNCrypt's supported subtypes last, because selecting type 19 commits the connection. It never
selects a sign-in. `main.rs` dispatches on `BridgeMode`. Tests: `tests/vnc_inspect_integration.rs`
(5).
