# 2026-10-21 — the extract-into-function assist cannot spell an opaque parameter type (E0121)

**Category:** Future enhancement (engine gap, rust-analyzer assist)
**Source:** `#host-desktops` 10/10, [#693](https://github.com/uppin/tddy-coder/pull/693) — plan `26-rdp-connect`

## What happened

Plan 26 asked for two `extract_method`s inside `RdpClient::connect_impl` (`packages/tddy-rdp/src/rdp_client.rs`).
`finalize_connection` (the `let server_public_key …` → `})?;` range) was refused by `restructure check --deep`, twice,
against a warm index; nothing was written:

```
0: rust-analyzer's answer was unusable: rust-analyzer wrote `async fn finalize_connection(host: &str, connector: ClientConnector, server_cert: _, framed: &mut ironrdp_tokio::Framed<ironrdp_tokio::TokioStream<ironrdp_server::tokio_rustls::client::TlsStream<TcpStream>>>, upgraded: ironrdp_tokio::Upgraded) -> Result<ironrdp_connector::ConnectionResult, anyhow::Error> {` — it produced the extraction before it could infer the types the signature needs, and `_` is not legal there (E0121). The crate graph was most likely still loading; retrying the operation against a warm server resolves it.
```

`server_cert` is the second element of the tuple `ironrdp_tls::upgrade(stream, host)` returns, a type the assist
cannot spell, so it writes `_`. The refusal's own hint (a cold index) does not apply: the daemon was warm. A
different class from the `*format!` and unused-generic entries: the assist's signature is not legal at all.

## Workaround used

Dropped: only `connector_config` was applied (developer decision 2026-10-21). `connect_impl` stays long; the rest
of the extraction remains in `2026-10-20-rdp-connect-config-builder-extraction.md`.

## Fix

Spell the type (`ironrdp_tls::TlsCertificate`-style, or `impl`) in the engine when the assist writes `_`, or
narrow the range so the opaque value stays inside it (for example, take `server_public_key` out of the
range's inputs by extracting from `ironrdp_tls::upgrade` onward).
