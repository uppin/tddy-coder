# 2026-10-20 — RDP NLA sign-in with a domain

**Type:** Feature

`#host-desktops` 2/8 ([#654](https://github.com/uppin/tddy-coder/pull/654)). The RDP bridge signs in to
servers that require NLA (CredSSP with NTLM) and carries a domain; one credential value replaces the
positional username and password across both bridges.

## What changed

- **`tddy-screenshare`.** `Credentials { username, domain, password }` (all `Option<String>`, `Default`)
  and `ScreenSharingClient::connect(host, port, &Credentials)`. `BridgeConfig.domain` is
  `#[serde(default)]`; `BridgeConfig::credentials()` maps each empty field to `None`, and `run_bridge`
  builds its credentials from it.
- **`tddy-rdp`.** CredSSP is enabled and the domain is passed to IronRDP; TLS or HYBRID follows the
  server's negotiation. `NoKdcNetworkClient` answers NTLM's no-network case and refuses a KDC request
  with an error. A CredSSP failure surfaces from `connect` with the context `NLA authentication failed`.
- **`tddy-vnc`.** Adapts to the new signature (reads `credentials.password`); no behaviour change.
- **Tests.** `tests/rdp_nla_integration.rs` drives `tddy-rdp` against an in-process `ironrdp-server` as a
  CredSSP acceptor (NLA, wrong password) and as a TLS-only server. Three `BridgeConfig` unit tests pin
  parsing, the domain and empty-to-absent.
- **Feature doc.** [Screen sharing sessions](../ft/web/screen-sharing-sessions.md#rdp-sign-in).

## Not changed

No daemon, proto or web field for the domain. No classification of authentication failures for the
daemon, no inspect mode, no VNC security negotiation.

## Final measurements

Scoped to `tddy-rdp`, `tddy-screenshare`, `tddy-vnc`: 17 tests pass, `clippy -D warnings` clean.
Production lines: `rdp_client.rs` 357, `bridge.rs` 301, `client.rs` 58, `vnc_client.rs` 316, all under the
500 budget. `RdpClient::connect` is 112 lines (107 before). The `ironrdp-server` acceptor does not check the
CredSSP domain, so the domain is pinned through the TLS-only desktop, which compares the whole Client Info.

## Deferred

[2026-10-20-rdp-connect-config-builder-extraction.md](../todo/2026-10-20-rdp-connect-config-builder-extraction.md),
[2026-10-20-rdp-empty-tls-public-key-fallback.md](../todo/2026-10-20-rdp-empty-tls-public-key-fallback.md),
[2026-10-20-rdp-test-support-module.md](../todo/2026-10-20-rdp-test-support-module.md).
