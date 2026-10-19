# Changeset: RDP desktops that require NLA sign in, with a domain

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**PR**: [#654](https://github.com/uppin/tddy-coder/pull/654)
**Type**: Feature
**Stack**: `#host-desktops` 2/8. Base: `feature/host-desktops/service-split`.

## Initial Discovery

[2026-10-18-host-desktops-rdp-nla-initial-discovery.md](./2026-10-18-host-desktops-rdp-nla-initial-discovery.md)

## Responsibility

- `tddy_screenshare::Credentials { username, domain, password }` (all `Option<String>`) and the
  `ScreenSharingClient::connect(host, port, &Credentials)` signature — owned here, consumed by node 3
  and node 8.
- `BridgeConfig.domain` (`#[serde(default)]`) and `run_bridge` building `Credentials` from the config.
- `tddy-rdp`: CredSSP enabled with a real `NetworkClient` (NTLM, no KDC), the domain passed to
  IronRDP, and the security protocol following the server's negotiation (TLS or HYBRID).
- `tddy-vnc`: the mechanical adaptation of its `connect` to the new signature (no behaviour).

## Boundaries

- Does not add any daemon, proto or web field for the domain — node 8 fills it.
- Does not classify authentication failures for the daemon (node 6's readiness protocol). This node
  makes an NLA refusal surface as an error from `connect`; node 6 maps it.
- Does not add an inspect mode (node 8).
- Does not touch VNC security negotiation (node 3).

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `service-split` | module layout of `tddy-screen-sharing` | nothing (line predecessor only) | touch `tddy-screen-sharing` |

## Draft PR contract

```rust
// packages/tddy-screenshare/src/client.rs
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Credentials {
    pub username: Option<String>,
    pub domain: Option<String>,
    pub password: Option<String>,
}
pub trait ScreenSharingClient {
    async fn connect(host: &str, port: u16, credentials: &Credentials) -> anyhow::Result<Self>;
    // … unchanged …
}
// packages/tddy-screenshare/src/bridge.rs
pub struct BridgeConfig { …, #[serde(default)] pub domain: String, … }
impl BridgeConfig { pub fn credentials(&self) -> Credentials; }   // empty string → None
```

Both bridges compile against the new signature in the contract commit (tddy-vnc's body unchanged
apart from reading `credentials.password`); tddy-rdp's NLA body is `todo!()`-free but still
TLS-only until green. Failing tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — the tests drive `tddy-rdp` against an in-process `ironrdp-server` and `BridgeConfig` parsing; nothing from another node.
**Concurrent with:** nodes 1 (after #626), 3, 4, 5
**Blocks:** node 3 `vnc-security` consumes the `Credentials` **signature** (available from this node's contract commit — not a behaviour wait); node 8 `sign-in-form` offers NLA only once it works here.

    #626 → n1      n1 → n6, n7, n8      n2 ⇢ n3 (signature only)      n2, n3, n6 → n8

## Affected Packages

- `tddy-screenshare` — [README](../../../packages/tddy-screenshare/README.md)
- `tddy-rdp` — [README](../../../packages/tddy-rdp/README.md)
- `tddy-vnc` — [README](../../../packages/tddy-vnc/README.md) (signature only)

## Related Feature Documentation

- PRD: `docs/ft/web/1-WIP/PRD-2026-10-18-host-desktops-rdp-nla.md`

## Summary

Windows desktops with NLA on (the default) can be opened; the bridge credential carries a domain.

## Scope

- [x] `Credentials` + new `connect` signature; both bridges adapted
- [x] `BridgeConfig.domain`, back-compatible
- [x] tddy-rdp CredSSP (NTLM) with domain; protocol from negotiation
- [x] NLA test server in the integration suite

## Technical Changes

### State A
- `packages/tddy-screenshare/src/client.rs:10-15` `connect(host, port, username: Option<&str>, password: Option<&str>)`.
- `packages/tddy-screenshare/src/bridge.rs:30-62` `BridgeConfig` has `username` (`#[serde(default)]`) and `password`; `run_bridge` maps empty → `None`.
- `packages/tddy-rdp/src/rdp_client.rs:31-41` `NoNetworkClient` ("no CredSSP is performed in TLS-only mode"); L64-109 `Config { credentials: UsernamePassword, enable_tls: true, enable_credssp: false, domain: None, … }`.
- `packages/tddy-rdp/tests/rdp_client_integration.rs`: `ironrdp-server` with `with_no_client_auth`.
- IronRDP: `ironrdp-connector 0.9`, `ironrdp-tokio 0.9`, `ironrdp-server 0.12` (dev).

### State B
`connect` takes `&Credentials`; tddy-rdp negotiates HYBRID when the server requires it and runs
CredSSP via `ironrdp_tokio::connect_finalize` with a NetworkClient that needs no network for NTLM.

### Delta
| Package | Change |
|---|---|
| `tddy-screenshare` | `client.rs` `Credentials` + signature; `bridge.rs` `domain`, `credentials()` |
| `tddy-rdp` | `rdp_client.rs` CredSSP, domain; `tests/rdp_nla_integration.rs` (new) |
| `tddy-vnc` | `vnc_client.rs` signature adaptation |

## Implementation Milestones
- [x] M1 Signature + config (contract commit), both bridges build
- [x] M2 NLA sign-in against the test server
- [x] M3 Wrong-password and TLS-only regressions green

## Testing Plan

Integration level, in-process: `ironrdp-server` configured for HYBRID (CredSSP) with known
credentials; the client signs in and receives a frame. Unit tests for `BridgeConfig` parsing and
`credentials()`. If `ironrdp-server 0.12` cannot act as a CredSSP acceptor, stop and raise it —
do not test NLA against a mock of IronRDP itself.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `signs_in_to_a_server_that_requires_nla_with_username_domain_and_password` | `packages/tddy-rdp/tests/rdp_nla_integration.rs` (new) | CredSSP sign-in streams a frame |
| `a_wrong_password_on_an_nla_server_fails_to_connect_as_an_authentication_error` | same | refusal is an error from `connect`, not a hang |
| `still_signs_in_to_a_server_that_offers_only_tls` | same | TLS-only path preserved |
| `a_bridge_config_without_a_domain_still_parses` | `packages/tddy-screenshare/src/bridge.rs` (`tests`) | back-compat of stdin JSON |
| `empty_config_fields_become_absent_credentials` | same | `""` → `None` for each field |
| `a_bridge_config_with_a_domain_carries_it_into_the_credentials` | same | `domain` reaches `credentials()` (added in the contract commit) |

Contract-commit notes:
- `ironrdp-server 0.12` **can** act as the CredSSP acceptor: `RdpServer::builder().with_hybrid(tls_acceptor, tls_public_key)` plus `server.set_credentials(..)` runs `ironrdp_acceptor::accept_credssp` (NTLM via `sspi`). It matches the account by name and password only — the domain sent in CredSSP is not checked by the acceptor — so the domain is pinned on the TLS-only path instead, where the acceptor compares the whole Client Info `Credentials { username, password, domain }`.
- The wrong-password test pins the refusal as an error whose `{:#}` chain contains `authentication`; node 6 maps that error.
- `run_bridge` builds its credentials from `BridgeConfig::credentials()`.

## Decisions & Trade-offs
- **One `Credentials` value** instead of growing positional `Option<&str>` parameters: node 3 needs the username, node 8 the domain, and a fourth positional would be unreadable.
- **NTLM only** — no KDC lookup; Kerberos stays out of scope (umbrella PRD).

## Technical Debt & Production Readiness

## Refactoring Needed
### From @red (TDD Red Phase)
- `tests/rdp_nla_integration.rs` duplicates the QR display and TLS helpers of `tests/rdp_client_integration.rs`; extract a shared `tests/support/` fake-desktop module once both suites are green.
### From /validate-changes
### From /validate-tests
- Optional: a wrong-domain refusal test on the TLS-only desktop; shared `tests/support/` module (same item as From @red).
### From /validate-prod-ready
- Warning: `.unwrap_or(&[])` on the TLS server public key (`rdp_client.rs`) masks a missing key now that CredSSP binds to it — carried over from the base; needs consent to change.
### From /analyze-clean-code
- `RdpClient::connect` is 112 lines (must refactor, pre-existing +5): extract the connector `Config` builder. Deferred to a follow-up after the stack lands (node 8 edits the same crate).

## Validation Results
### /validate-changes
**Run 2026-10-20, base `feature/host-desktops/service-split`** (already current, leak check clean: 4 own commits). Scoped to `tddy-rdp`, `tddy-screenshare`, `tddy-vnc`.

- Build: `cargo check --all-targets` ✅ for all three; `cargo test` ✅ (rdp_client 5, rdp_nla 3, screenshare 6, vnc 5).
- Changeset sync: all six acceptance tests exist and pass; scope and milestones M1–M3 complete.
- Boundaries: ✅ no daemon/proto/web field, no failure classification (the error carries `NLA authentication failed` for node 6 to map), no VNC negotiation change, `tddy-screen-sharing` untouched (its stdin JSON has no `domain` key and still parses).
- No `todo!`/`unimplemented!`/`TODO(rdp-nla)` left in the three packages.
- [INFO] `Credentials` derives `Debug` with the password in clear (per the draft contract). Nothing logs it today; node 3/8 must not format it into logs.
- [INFO] `ConnectorErrorKind::Credssp(_)` is labelled "NLA authentication failed" for every CredSSP failure, not only a bad password; node 6 owns the finer mapping.
- [INFO] The `ironrdp-server` acceptor ignores the CredSSP domain, so the NLA test proves the domain is passed through, not enforced.
- [INFO] `NoKdcNetworkClient::send` builds an `Err` then `inspect_err`s it; a plain `log::warn!` then `return Err` reads simpler.
### /validate-tests
**Run 2026-10-20** — 14 tests analyzed (rdp_nla 3, bridge 3, rdp_client 5 adapted, vnc 5 untouched); 0 critical, 0 warning, 3 info.

- Fluent-tests: ✅ Given/When/Then in every new test, one behaviour each, named builders (`a_desktop_that_requires_nla`, `alices_credentials`), semantic fixtures, no `#[ignore]`, no sleeps in assertions.
- Description-body alignment: ✅ all six acceptance tests assert what their names say; the wrong-password test pins the `authentication` chain and a bounded sign-in so a hang fails rather than blocks.
- Flakiness: ports bound on `:0`; the 10 s sign-in and 5 s frame bounds are named hang guards, not timing assertions.
- [INFO] The domain is pinned only through the TLS-only desktop (the acceptor ignores it under CredSSP); there is no explicit "wrong domain is refused" test.
- [INFO] No test for a partly filled `Credentials` (username without password) at the `BridgeConfig` seam.
- [INFO] `rdp_nla_integration.rs` still duplicates the QR and TLS helpers of `rdp_client_integration.rs` (see Refactoring Needed, From @red).
### /validate-prod-ready
**Run 2026-10-20** — ⚠️ Gaps (0 blockers, 1 warning, 2 info). 4 production files checked (tests excluded).

| Category | Count | Status |
|----------|-------|--------|
| Mock code | 0 | ✅ |
| Dev fallbacks | 1 (carried over) | ⚠️ |
| TODO/FIXME | 0 (both `TODO(rdp-nla)` markers resolved) | ✅ |
| Unused code | 0 | ✅ |
| Debug output | 0 (`log::warn!` only; no `println!`/`dbg!`) | ✅ |

- [WARNING] `packages/tddy-rdp/src/rdp_client.rs` `extract_tls_server_public_key(..).unwrap_or(&[])` — carried over from the base, not added here. With CredSSP on, an empty key no longer just works: CredSSP binds to the server's TLS key, so a certificate that yields no key surfaces as a confusing NLA authentication failure. Fix (separate, behaviour-changing): refuse with a clear error when NLA is negotiated. Not changed here; needs developer consent as a behaviour change on the TLS-only path.
- [INFO] `username/password .unwrap_or_default()` into the IronRDP `Credentials` — carried over (`unwrap_or("")`); absent RDP credentials are sent as empty strings and the server refuses them.
- [INFO] `NoKdcNetworkClient` returns an error rather than an empty reply, so Kerberos is refused loudly (no silent fallback).
### /analyze-clean-code
**Run 2026-10-20** — Score **C** (1 must-refactor, pre-existing; 0 needs-attention). 5 production files.

- File length: ✅ all ≤ 500 production lines (largest `rdp_client.rs` 357; `bridge.rs` 301).
- Must refactor: `packages/tddy-rdp/src/rdp_client.rs` `RdpClient::connect` — **112 lines** (base 107, +5 from this PR: CredSSP flag, domain, error-stage mapping). Pre-existing; this PR grew it slightly. Suggested split: `connector_config(&Credentials) -> Config` and the TLS-upgrade/finalize steps. **Deferred**: node 8 `sign-in-form` also edits `tddy-rdp/src`, so restructuring this file here would conflict with it; split in a follow-up after the stack lands.
- `run_bridge` 68 lines (base 79): this PR shortened it by moving the credential mapping into `BridgeConfig::credentials()`.
- Nesting ≤ 3, parameters ≤ 3 everywhere touched.
- Magic values: none added (256x256, 0x0409 are carried over).
- Duplication: none in production code; test-helper duplication tracked under Refactoring Needed (From @red).
- Applied: `NoKdcNetworkClient::send` now logs then returns the error (replaces `Err(..).inspect_err`).

## TODO

- [x] Record initial discovery (`2026-10-18-host-desktops-rdp-nla-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (approved with the wave-1 batch, 2026-10-18)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Run scoped tests for touched packages (tddy-rdp, tddy-screenshare, tddy-vnc: 17 pass); full-workspace result from CI, post-wrap
- [x] Validate changes (/validate-changes)
- [x] Validate tests (/validate-tests)
- [~] Validate production readiness (/validate-prod-ready) — 1 warning awaiting approval: carried-over `unwrap_or(&[])` on the TLS public key
- [~] Analyze code quality (/analyze-clean-code) — score C: `RdpClient::connect` 112 lines, split deferred (node 8 edits the same crate); awaiting approval
- [x] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-host-desktops-rdp-nla-initial-discovery.md`
- [ ] USER REVIEW — work complete
