# 2026-10-20 — `RdpClient::connect` is 112 lines

**Category:** Deferred from `host-desktops-rdp-nla` (#654)
**Source:** `/analyze-clean-code` on #654 (score C), deferral approved by the developer.

`packages/tddy-rdp/src/rdp_client.rs` `RdpClient::connect` is 112 lines (107 before #654, which added the
CredSSP flag, the domain and the error-stage mapping). Extract the connector `Config` builder
(`connector_config(&Credentials) -> Config`) and the TLS-upgrade/finalize steps.

**Why deferred:** `#host-desktops` node 8 (`sign-in-form`) also edits `packages/tddy-rdp/src`, so restructuring the
file inside the stack would conflict with it. Do it as a follow-up once the stack has landed. It is a
mechanical extract; no behaviour change.

**Narrowed (2026-10-21, `#host-desktops` 10/10, [#693](https://github.com/uppin/tddy-coder/pull/693)):** half done.
The trait body moved by consented hand move into the inherent `RdpClient::connect_impl` (the trait's `connect`
is a one-line delegator), and the engine extracted `connector_config(&tddy_screenshare::Credentials) -> Config`
(46 lines). **`connect_impl` is 72 lines** (measured at #693's head, signature through closing brace), still over
the 60-line line.

**What remains:** the TLS-upgrade/finalize step — `let server_public_key …` through the `connect_finalize(…)
.await.map_err(…)?;` — as a `finalize_connection` helper. The engine refused it (`restructure check --deep`,
twice, warm index): the extract assist writes `server_cert: _` for the certificate `ironrdp_tls::upgrade`
returns, a type it cannot spell (E0121). See
[2026-10-21-restructure-extract-assist-cannot-spell-an-opaque-parameter-type.md](2026-10-21-restructure-extract-assist-cannot-spell-an-opaque-parameter-type.md).
Retry once that is fixed, or with a range that keeps `server_cert` inside it (from `ironrdp_tls::upgrade`
onward), which would bring `connect_impl` to about 50 lines.
