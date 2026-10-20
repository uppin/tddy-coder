# Changeset: VeNCrypt and Apple Remote Desktop sign-in for VNC desktops

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**PR**: [#655](https://github.com/uppin/tddy-coder/pull/655)
**Type**: Feature
**Stack**: `#host-desktops` 3/8. Base: `feature/host-desktops/rdp-nla`.

## Initial Discovery

[2026-10-18-host-desktops-vnc-security-initial-discovery.md](./2026-10-18-host-desktops-vnc-security-initial-discovery.md)

## Responsibility

`packages/tddy-vnc/src/security.rs` (new): RFB version handshake, security-type list parsing with
unknown types preserved, method selection given the available credentials, and the exchanges for
None, VNC Authentication, VeNCrypt Plain, VeNCrypt TLSPlain and Apple Remote Desktop (type 30).
The public parser `parse_security_types` and the `VncSecurityType` enum are owned here and reused by
node 8's inspect mode. `VncClientState::connect` runs this layer, then hands the authenticated
stream to vnc-rs from ServerInit (or a vnc-rs patch pinned by git rev if no such entry exists —
decided in green, recorded in Decisions).

## Boundaries

- Does not change `Credentials` or the `connect` signature (node 2 owns them).
- Does not report authentication outcomes to the daemon (node 6); a refused SecurityResult is an
  error from `connect` carrying the server's reason.
- Does not add an inspect mode or CLI flag (node 8).
- **Does not add a crate without asking** — ARD needs Diffie-Hellman over a bignum, AES-128-ECB and
  MD5; each new dependency is confirmed with the developer first (CLAUDE.md ASK).

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n2 `rdp-nla` | `Credentials { username, domain, password }`, `connect(host, port, &Credentials)` | reads `username` / `password` to choose and run a method | change the struct or trait, or touch tddy-rdp |
| n1 `service-split` | — | nothing (line predecessor) | — |

Only n2's **signature** is consumed; it exists from n2's contract commit, so this node is greenable
without n2 being green.

## Draft PR contract

```rust
// packages/tddy-vnc/src/security.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VncSecurityType { None, VncAuth, VeNCrypt, AppleRemoteDesktop, Unknown(u8) }
pub fn parse_security_types(bytes: &[u8]) -> Result<Vec<VncSecurityType>, SecurityError>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VncSignIn { None, VncAuth, VeNCryptPlain, VeNCryptTlsPlain, AppleRemoteDesktop }
pub fn choose_sign_in(offered: &[VncSecurityType], vencrypt_subtypes: &[u32], credentials: &Credentials) -> Result<VncSignIn, SecurityError>;
#[derive(Debug, thiserror::Error)]
pub enum SecurityError { Refused { reason: String }, NothingSupported { offered: Vec<u8> }, MissingCredential(&'static str), Io(#[from] std::io::Error), Protocol(String) }
```

Bodies `todo!()` until green, except where trivially pure. Failing tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — tests run `tddy-vnc` against an in-process fake RFB server; n2's signature is already published.
**Concurrent with:** nodes 1 (after #626), 2, 4, 5
**Blocks:** node 8 `sign-in-form` (its VNC inspect reuses `parse_security_types`, and offering VeNCrypt/ARD in the form needs them to work)

    #626 → n1      n1 → n6, n7, n8      n2 ⇢ n3 (signature only)      n2, n3, n6 → n8

## Affected Packages

- `tddy-vnc` — [README](../../../packages/tddy-vnc/README.md)

## Related Feature Documentation

- PRD: `docs/ft/web/1-WIP/PRD-2026-10-18-host-desktops-vnc-security.md`

## Summary

macOS Screen Sharing and VeNCrypt-only servers (TigerVNC's default) can be opened.

## Scope

- [x] Version + security negotiation in `tddy-vnc`
- [x] VncAuth, VeNCrypt Plain/TLSPlain, ARD exchanges
- [x] Hand-off to vnc-rs from ServerInit (or pinned patch)
- [x] Fake RFB server in tests advertising configurable security lists

## Technical Changes

### State A
- `packages/tddy-vnc/src/vnc_client.rs:22-42` `VncConnector::new(tcp).set_auth_method(async move { Ok(password) })`; L279-287 `connect(host, port, _username, password)` — username ignored.
- vnc-rs `ab684d0` `client/connector.rs:52-135`: only None or VncAuth, else `"Security type apart from Vnc Auth has not been implemented"`; `client/auth.rs:75-77` unknown type numbers fail the whole list.
- `packages/tddy-vnc/tests/vnc_client_integration.rs`: fake server advertising only `SecurityType::None`.

### State B
`connect` → TCP → `security::negotiate(stream, &credentials)` → (TLS-wrapped for TLSPlain) → vnc-rs
from ServerInit → existing frame/input path.

### Delta
| Package | Change |
|---|---|
| `tddy-vnc` | `src/security.rs` (new), `src/vnc_client.rs`, `tests/vnc_security_integration.rs` (new), `Cargo.toml` (crates — ask first) |

## Implementation Milestones
- [x] M1 Parser + chooser (pure, unit-tested)
- [x] M2 VncAuth and VeNCrypt Plain against the fake server; hand-off decided
- [x] M3 TLSPlain and ARD

## Testing Plan

Unit tests for the parser and chooser in `security.rs`. Integration tests with a fake RFB server
that advertises a configurable list, performs the server side of each exchange with known
credentials, then serves ServerInit and one framebuffer update — so "signs in" means a frame
streamed, not only a SecurityResult. ARD's server side is computed in the test from the same DH
parameters (RFC-documented algorithm), not by calling the client's code.

## Acceptance Tests

| Test | File | Validates | Status |
|---|---|---|---|
| `signs_in_with_vnc_authentication` | `packages/tddy-vnc/tests/vnc_security_integration.rs` (new) | VncAuth then a frame | [x] written — **passes today** through vnc-rs; regression guard for the new layer |
| `signs_in_with_vencrypt_plain` | same | username + password over Plain | [x] written, failing (`VNC handshake failed`) |
| `signs_in_with_vencrypt_tls_plain` | same | Plain inside TLS | [x] written, failing (`VNC handshake failed`); fake server offers Plain + TLSPlain, records the chosen subtype, accepts with `1`, then runs Plain inside rustls with an rcgen self-signed cert |
| `signs_in_with_apple_remote_desktop` | same | type 30 DH exchange | [x] written, failing (`VNC handshake failed`); fake server uses RFC 2409 group 2 (g=2, 128-byte key), derives the shared secret from its own private key, MD5 → AES-128-ECB decrypts the 128-byte block and checks both null-terminated fields |
| `chooses_vnc_authentication_when_no_username_is_given_and_ard_is_also_offered` | same | method choice by credentials | [x] written, failing (`VNC handshake failed` — vnc-rs rejects type 30) |
| `chooses_apple_remote_desktop_when_a_username_is_given` | same | method choice by credentials | [x] written, failing (`VNC handshake failed` — vnc-rs rejects type 30); asserts type 30 was selected **and** a frame after a completed ARD exchange |
| `a_server_offering_only_unknown_types_is_refused_naming_them` | same | error names the numbers | [x] written, failing (`Unknow VNC security type: 77` — 99 not named) |
| `a_refused_sign_in_fails_to_connect_with_the_servers_reason` | same | SecurityResult failure reason surfaced | [x] written — **passes today** (vnc-rs surfaces the reason); regression guard |

The fake server's VNC Authentication check uses a known-answer vector (DES of a fixed challenge
under the key for `secret`, computed with `openssl enc -des-ecb`), so no DES crate is needed in
tests; `signs_in_with_vnc_authentication` passing against vnc-rs confirms the vector. The ARD and
TLSPlain server sides were checked once with a throwaway client written against the fake server
(not committed): both reached SecurityResult OK and ServerInit.

Unit (`src/security.rs`): [x] `parses_a_list_that_mixes_known_and_unknown_types`,
[x] `a_method_needing_a_username_is_not_chosen_without_one`, [x] `prefers_tls_plain_over_plain` —
all written, failing on `todo!()`. `parse_security_types` takes the whole security-types message
(count byte, then the types).

## Decisions & Trade-offs
- **TLSPlain accepts an unverified server certificate** (developer, 2026-10-18). rustls has no anonymous-DH suites, so TLSPlain works with servers that present a certificate (verified or not) and not with anonymous-only ones (e.g. GnuTLS anon). Same posture as tddy-rdp's TLS; documented as a limit at wrap.
- **Crates approved** (developer, 2026-10-18), at the versions already in Cargo.lock (no version moves): normal deps `rustls` 0.23 and `tokio-rustls` 0.26 (both `default-features = false`, `ring` + `tls12` + `logging`, matching tddy-livekit's ring choice), `num-bigint` 0.4, `aes` 0.8, `md-5` 0.10, `thiserror` 2; dev-dep `rcgen` 0.13.
- **TLSPlain trust model — decided in green:** the client accepts an unverified server certificate for TLSPlain (it still checks the TLS handshake signature against the presented certificate's key). That interoperates with certificate-presenting servers but not with servers offering anonymous-DH suites only (TigerVNC/GnuTLS default), which rustls does not implement. X509Plain (262) would be the verified variant. Documented in the `security.rs` module docs.
- **Own security layer rather than a vnc-rs fork** (developer, 2026-10-18). Fallback to a pinned patch only if vnc-rs cannot be entered after SecurityResult — recorded here when decided.
- **Hand-off to vnc-rs — decided in green: no patch needed.** vnc-rs `ab684d0` has no entry point after SecurityResult (its connector always runs the version and security handshake). `security::negotiate` returns a `SecuredStream` that replays the handshake bytes vnc-rs expects to read and swallows the 12-byte version echo it writes. VeNCrypt and ARD are run in this layer, then the stream replays RFB 3.3 + security type None, so vnc-rs reads nothing before ClientInit. VNC Authentication and None are left to vnc-rs over the same stream (replayed version + the single chosen type), because no DES crate is a dependency. Consequence: a refused VNC Authentication reaches the caller as vnc-rs's error carrying the server's reason, not as `SecurityError::Refused`; VeNCrypt/ARD refusals are `SecurityError::Refused`. Swapping this for in-layer DES needs a `des` crate (ask first).
- **VeNCrypt ack byte:** the fake server sends an accept byte after the subtype choice only for TLSPlain, not Plain; the client matches that. TODO: confirm against a real TigerVNC server whether Plain also sends one.
- **Inherent `VncClientState::connect(host, port, password)` kept** (the existing integration tests call it); `connect_with_credentials` carries the full `Credentials` and is what the `ScreenSharingClient` trait impl uses.

## Technical Debt & Production Readiness

## Refactoring Needed
### From @red (TDD Red Phase)
- The fake RFB server in `vnc_security_integration.rs` duplicates the ServerInit/event loop of the one in `vnc_client_integration.rs`; a shared `tests/common` fake server would serve both.

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
2026-10-20, after `/pr-stack-rebase` onto `rdp-nla` `3330f1205` (leak check clean: 4 own commits).
Scoped build/test/clippy/fmt for `tddy-vnc`: 16/16 tests pass, clippy and fmt clean.
Boundaries held: `Credentials` and the trait signature untouched, no tddy-rdp edit, no daemon
outcome reporting, no inspect mode, only the approved crates added. No `todo!()` left in
`security.rs`. Changeset items: all three milestones ✅ implemented.

- ✅ (fixed 2026-10-20, regression test `a_vencrypt_server_with_no_supported_subtype_is_refused_without_a_second_type_byte`) **VeNCrypt commitment can fall through to another method.** `negotiate` writes type 19
  *before* `choose_sign_in`; if the server's VeNCrypt subtypes hold neither Plain nor TLSPlain and
  it also offers ARD / VNC Authentication / None, `choose_sign_in` returns that method and the
  client writes a second type byte into the VeNCrypt phase (`security.rs` `negotiate`). It must
  fail with `NothingSupported` naming `[19]` once VeNCrypt is chosen.
- ✅ (fixed 2026-10-20: `log::warn!` + module docs, behaviour unchanged) **VeNCrypt Plain sends the password in the clear** when it is the only subtype offered, with
  no log line. Not a fallback to add silently; at least `log::warn!` and name it in the docs.
- ⚠️ `security.rs` is 697 production lines (budget 500) — step 3.5 of `/pr-wrap`.
- ℹ️ `negotiate` has no timeout around its reads; a silent server stalls `connect`.
- ℹ️ ARD accepts a zero `generator`; harmless but unvalidated.
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-18-host-desktops-vnc-security-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests (8 of 8)
- [x] Run acceptance tests (verify they fail) — 6 fail, 2 pass today as regression guards
- [x] USER REVIEW — acceptance tests (approved with the wave-1 batch, 2026-10-18)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-host-desktops-vnc-security-initial-discovery.md`
- [ ] USER REVIEW — work complete
