# 2026-10-20 — VNC security negotiation: VeNCrypt and Apple Remote Desktop

**Type:** Feature

`#host-desktops` 3/8 ([#655](https://github.com/uppin/tddy-coder/pull/655)). `tddy-vnc` runs the RFB
version and security negotiation itself and signs in with None, VNC Authentication, VeNCrypt Plain,
VeNCrypt TLSPlain and Apple Remote Desktop (type 30). Product behaviour:
[VNC sign-in](../../../../docs/ft/web/screen-sharing-sessions.md#vnc-sign-in).

## Module layout

`security.rs` holds the module docs, constants, the `negotiate*` orchestration, the RFB version
handshake, sign-in dispatch, `pub use` re-exports and the unit tests. Children under `security/`:

| File | Production lines | Holds |
|------|------:|------|
| `security.rs` | 398 (was 889 in one file, budget 500) | as above |
| `security/types.rs` | 174 | `VncSecurityType`, `VncSignIn`, `SecurityError`, `parse_security_types`, `choose_sign_in` |
| `security/stream.rs` | 83 | `Transport`, `SecuredStream` and its `AsyncRead`/`AsyncWrite` impls |
| `security/vencrypt.rs` | 149 | subtype exchange, Plain credentials, `start_tls`, the unverified-certificate verifier |
| `security/ard.rs` | 139 | Apple Remote Desktop exchange, AES/MD5 key derivation |

Made with `tddy-tools restructure` (four `move_item` operations, `reexport: named`; `check --deep
--budget 500` clean); no moved code was written by hand. Every public path is unchanged
(`tddy_vnc::security::{VncSecurityType, VncSignIn, SecurityError, parse_security_types,
choose_sign_in, negotiate, SecuredStream, Transport}`). The parent stays `security.rs` beside a
`security/` directory because the engine does not rename files. The unit tests stay in `security.rs`:
they exercise items from all four children. `parse_security_types` takes the whole security-types
message (count byte, then the types) and is public for reuse.

## Hand-off to vnc-rs, with no patch

The vnc-rs revision in use has no entry point after SecurityResult: its connector always runs the
version and security handshake. `security::negotiate` therefore returns a `SecuredStream` that
replays the handshake bytes vnc-rs expects to read and swallows the 12-byte version echo it writes.
VeNCrypt and Apple Remote Desktop run in this layer, after which the stream replays RFB 3.3 and
security type None, so vnc-rs reads nothing before ClientInit. VNC Authentication and None are left
to vnc-rs over the same stream (replayed version and the single chosen type), because no DES crate is
a dependency. Inherent `VncClientState::connect(host, port, password)` is kept;
`connect_with_credentials` carries the full `Credentials` and backs the `ScreenSharingClient` impl.

## Decisions

- **TLSPlain accepts an unverified server certificate** (it still checks the handshake signature
  against the presented key), the same posture as `tddy-rdp`'s TLS. rustls has no anonymous-DH suites,
  so servers offering only those are not supported. X509Plain (262) would be the verified variant.
- **Crates approved by the developer**, at the versions already locked: `rustls` 0.23 and
  `tokio-rustls` 0.26 (`default-features = false`, `ring` + `tls12` + `logging`), `num-bigint` 0.4,
  `aes` 0.8, `md-5` 0.10, `thiserror` 2; dev-dependency `rcgen` 0.13.
- **Once VeNCrypt is chosen, no other method is tried**: a list with neither Plain nor TLSPlain fails
  with `NothingSupported` naming `[19]`.
- **VeNCrypt Plain over a clear channel** logs a warning and is named in the module docs.
- **30 second `SIGN_IN_TIMEOUT`** around the negotiation (`SecurityError::TimedOut`); ARD refuses a
  generator or public key below 2 and a server public key at or above `prime - 1`.

## Open limits

- A refused VNC Authentication is vnc-rs's error carrying the server's reason, not
  `SecurityError::Refused`; closing that needs a `des` crate, which is not approved.
- The accept byte after the VeNCrypt subtype choice (sent for TLSPlain, not for Plain) matches the
  test server and is unconfirmed against a real TigerVNC server.
- The fake RFB server in `tests/vnc_security_integration.rs` duplicates the ServerInit and event loop
  of the one in `tests/vnc_client_integration.rs`; a shared `tests/support` server is not done.
- CI has not yet run on this PR.

## Verification

Scoped to `tddy-vnc`: 23 unit, 5 `vnc_client_integration` and 16 `vnc_security_integration` tests pass;
`cargo clippy -p tddy-vnc --all-targets -- -D warnings` and `cargo fmt -p tddy-vnc` clean. No
`docs/dev/todo/` entries are resolved and `packages/tddy-vnc/docs/code-issues/` has no records.
