# Changeset: VeNCrypt and Apple Remote Desktop sign-in for VNC desktops

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**PR**: (opened in wave 1; number recorded in wave 2)
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

- [ ] Version + security negotiation in `tddy-vnc`
- [ ] VncAuth, VeNCrypt Plain/TLSPlain, ARD exchanges
- [ ] Hand-off to vnc-rs from ServerInit (or pinned patch)
- [ ] Fake RFB server in tests advertising configurable security lists

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
- [ ] M1 Parser + chooser (pure, unit-tested)
- [ ] M2 VncAuth and VeNCrypt Plain against the fake server; hand-off decided
- [ ] M3 TLSPlain and ARD

## Testing Plan

Unit tests for the parser and chooser in `security.rs`. Integration tests with a fake RFB server
that advertises a configurable list, performs the server side of each exchange with known
credentials, then serves ServerInit and one framebuffer update — so "signs in" means a frame
streamed, not only a SecurityResult. ARD's server side is computed in the test from the same DH
parameters (RFC-documented algorithm), not by calling the client's code.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `signs_in_with_vnc_authentication` | `packages/tddy-vnc/tests/vnc_security_integration.rs` (new) | VncAuth then a frame |
| `signs_in_with_vencrypt_plain` | same | username + password over Plain |
| `signs_in_with_vencrypt_tls_plain` | same | Plain inside TLS |
| `signs_in_with_apple_remote_desktop` | same | type 30 DH exchange |
| `chooses_vnc_authentication_when_no_username_is_given_and_ard_is_also_offered` | same | method choice by credentials |
| `chooses_apple_remote_desktop_when_a_username_is_given` | same | method choice by credentials |
| `a_server_offering_only_unknown_types_is_refused_naming_them` | same | error names the numbers |
| `a_refused_sign_in_fails_to_connect_with_the_servers_reason` | same | SecurityResult failure reason surfaced |

Unit (`src/security.rs`): `parses_a_list_that_mixes_known_and_unknown_types`,
`a_method_needing_a_username_is_not_chosen_without_one`, `prefers_tls_plain_over_plain`.

## Decisions & Trade-offs
- **Own security layer rather than a vnc-rs fork** (developer, 2026-10-18). Fallback to a pinned patch only if vnc-rs cannot be entered after SecurityResult — recorded here when decided.

## Technical Debt & Production Readiness

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-18-host-desktops-vnc-security-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-host-desktops-vnc-security-initial-discovery.md`
- [ ] USER REVIEW — work complete
