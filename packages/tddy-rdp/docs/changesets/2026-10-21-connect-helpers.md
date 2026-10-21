# 2026-10-21 — `RdpClient::connect` builds its connector config in `connector_config`

**Type:** Refactor

`#host-desktops` 10/10 ([#693](https://github.com/uppin/tddy-coder/pull/693)). No behaviour change. The
`ScreenSharingClient::connect` body (109 lines) was moved by hand, with the developer's consent and proved
byte-identical by script, into `RdpClient::connect_impl`; the trait method delegates. The engine then extracted
the connector `Config` literal as `connector_config(&Credentials) -> Config` (46 lines; returning the literal
directly is a hand lint fix for `clippy::let_and_return`). `connect` 115 ➜ `connect_impl` **72** lines: the
finalize step was refused by the engine (the assist cannot spell the TLS certificate's type, E0121), so the
backlog entry `2026-10-20-rdp-connect-config-builder-extraction` stays open, narrowed to it. `rdp_client.rs`
446 ➜ 459 production lines. Baseline: 11 passed / 0 failed before and after.
