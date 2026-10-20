# 2026-10-21 — RDP inspect reads the X.224 negotiation

**Type:** Feature

`#host-desktops` 8/8 ([#660](https://github.com/uppin/tddy-coder/pull/660)). `RdpClient::inspect` sends a Connection Request offering TLS and
CredSSP, and maps the Confirm's selection or failure code to `RdpNla` or `RdpTls`, bounded by 30 s.
`main.rs` dispatches on `BridgeMode`. Tests: `tests/rdp_inspect_integration.rs` (2).
