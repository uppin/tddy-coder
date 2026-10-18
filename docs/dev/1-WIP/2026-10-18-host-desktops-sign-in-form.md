# Changeset: Desktop sign-in shaped by its advertised methods, with Remember

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**PR**: (opened in wave 1; number recorded in wave 2)
**Type**: Feature
**Stack**: `#host-desktops` 8/8. Base: `feature/host-desktops/x11-channels`.

## Initial Discovery

[2026-10-18-host-desktops-sign-in-form-initial-discovery.md](./2026-10-18-host-desktops-sign-in-form-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-screen-sharing/docs/code-issues/oversized-file-screen-sharing-service.md` | ⚠ DURING | Inspect and the remembered-credential path go in node 1's host-target module or a new one, not back into the service file. |
| [2026-09-10-vnc-proto-has-no-server-and-a-live-web-client.md](../todo/2026-09-10-vnc-proto-has-no-server-and-a-live-web-client.md) | — not fixed here | Same area; the new dialog must not reuse `VncPassphraseDialog` / `vnc_*` (superseded generation). |

## Responsibility

- `screen_sharing.proto`: `rpc InspectHostDesktop`, `enum DesktopAuthMethod` (`NONE`, `VNC_PASSWORD`, `VENCRYPT_PLAIN`, `VENCRYPT_TLS_PLAIN`, `APPLE_REMOTE_DESKTOP`, `RDP_TLS`, `RDP_NLA`), `domain` on `ScreenSharingTarget` / `AddHostTargetRequest`.
- `host.proto`: `HostPromptKind.DESKTOP_CREDENTIALS`, the prompt's `repeated DesktopAuthMethod offered` and the encrypted answer's plaintext shape (`DesktopCredentialAnswer { method, username, domain, password, remember }`, protobuf-encoded before RSA-OAEP).
- Bridge **inspect mode** (`--inspect`, `{host,port}` on stdin, one JSON line out): `tddy_screenshare::run_inspect`, `ScreenSharingClient::inspect`, the tddy-vnc implementation over node 3's `parse_security_types` (and the VeNCrypt subtype list), the tddy-rdp implementation reading the X.224 Connection Confirm's selected/failure protocol.
- Daemon: `InspectHostDesktop`; `StartHostStream` using a remembered credential (vault record `screen-sharing` keyed `host:<instance>:<target>`) or prompting `DESKTOP_CREDENTIALS`; writing the record when `remember`; marking it refused on `Unauthenticated`.
- Web: `HostDesktopSignInDialog` (fields by method, method switcher, Remember), built on `HostPassphraseDialog`'s key-pinning/encryption; `HostDesktopOverlay` inspects first, skips the form for `NONE`, re-asks on `Unauthenticated`.

## Boundaries

- Does not change `Credentials` (node 2), negotiation (node 3), readiness (node 6) or channels (node 7).
- The periodic probe stays byte-free; inspection runs only on an operator's open.
- No accounts-screen editing of remembered host credentials.
- Session-scoped targets keep their own form.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `service-split` | host-target RPC module | adds inspect and the remembered path there | move modules |
| n2 `rdp-nla` | `Credentials.domain`, working NLA | the daemon fills `domain`; the form offers `RDP_NLA` | change CredSSP |
| n3 `vnc-security` | `parse_security_types`, VeNCrypt/ARD sign-in | VNC inspect reuses the parser; form offers those methods | re-implement parsing |
| n6 `bridge-ready` | `Unauthenticated` with the server's reason | re-asks; marks remembered credential refused | change the readiness protocol |
| n7 `x11-channels` | channel-addressed overlay, `display` targets | inspects the channel being opened (X11 channels report `NONE`) | change enumeration |

## Draft PR contract

```proto
// screen_sharing.proto
rpc InspectHostDesktop(InspectHostDesktopRequest) returns (InspectHostDesktopResponse);
message InspectHostDesktopRequest { string session_token = 1; string daemon_instance_id = 2; string target_id = 3; }
message InspectHostDesktopResponse { repeated DesktopAuthMethod offered = 1; repeated uint32 unsupported_vnc_types = 2; bool remembered = 3; }
enum DesktopAuthMethod { DESKTOP_AUTH_METHOD_UNSPECIFIED = 0; DESKTOP_AUTH_METHOD_NONE = 1; DESKTOP_AUTH_METHOD_VNC_PASSWORD = 2; DESKTOP_AUTH_METHOD_VENCRYPT_PLAIN = 3; DESKTOP_AUTH_METHOD_VENCRYPT_TLS_PLAIN = 4; DESKTOP_AUTH_METHOD_APPLE_REMOTE_DESKTOP = 5; DESKTOP_AUTH_METHOD_RDP_TLS = 6; DESKTOP_AUTH_METHOD_RDP_NLA = 7; }
message DesktopCredentialAnswer { DesktopAuthMethod method = 1; string username = 2; string domain = 3; string password = 4; bool remember = 5; }
// host.proto
enum HostPromptKind { …; HOST_PROMPT_KIND_DESKTOP_CREDENTIALS = 3; }
message HostPromptEvent { …; repeated int32 offered_auth_methods = 9; }
```
```rust
// tddy-screenshare
pub trait ScreenSharingClient { async fn inspect(host: &str, port: u16) -> anyhow::Result<Vec<AdvertisedMethod>>; … }
pub async fn run_inspect<C: ScreenSharingClient>(target: InspectTarget) -> anyhow::Result<()>;
```
```tsx
export function HostDesktopSignInDialog(props: { question: HostPromptEvent; offered: DesktopAuthMethod[]; onSubmit(answer: Uint8Array): void; onCancel(): void; error?: string }): JSX.Element;
```
Failing tests below.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** **no** — the re-ask test needs node 6's `Unauthenticated`; VNC inspect needs node 3's parser working; NLA in the form needs node 2 working; module placement needs node 1.
**Concurrent with:** none
**Blocks:** nothing.

    #626 → n1      n1 → n6, n7, n8      n2 ⇢ n3 (signature only)      n2, n3, n6 → n8

## Affected Packages

- `tddy-service` (protos), `tddy-screenshare`, `tddy-vnc`, `tddy-rdp`, `tddy-screen-sharing`, `tddy-host-service` (prompt kind), `tddy-web`

## Related Feature Documentation

- PRD: `docs/ft/web/1-WIP/PRD-2026-10-18-host-desktops-sign-in-form.md`

## Summary

The desktop asks for what it needs, and only once if the operator says so.

## Scope

- [ ] Protos + generated TS
- [ ] Inspect mode in both bridges
- [ ] Daemon inspect, remembered credential, prompt with method, remember write, refused marking
- [ ] Sign-in dialog; overlay inspect → form → start → re-ask

## Technical Changes

### State A
- `HostDesktopOverlay.tsx:71-95` adds targets with `username: ""`; `:242-265` one-field `HostPassphraseDialog` with `allowEmpty`.
- `screen_sharing_service.rs:266-299` `prompt_for_desktop_password` → `PromptKind::DesktopPassword`; `:616-625` decrypts a password string.
- `host.proto:274-282` prompt kinds `SSH_KEY_PASSPHRASE`, `DESKTOP_PASSWORD`.
- `tddy-screen-sharing/src/screen_sharing_records.rs:31` `SCREEN_SHARING_PROVIDER = "screen-sharing"`; session targets stored as vault records (metadata: label, host, port, protocol, username; secret: password).
- `remote_desktop_probe.rs` "A connect, and not one byte more" — unchanged.

### State B
Open → `InspectHostDesktop` → (remembered? start) : (form by method → encrypted `DesktopCredentialAnswer` → start) → `Unauthenticated`? re-ask with reason.

### Delta
| Package | Change |
|---|---|
| `tddy-service` | `screen_sharing.proto`, `host.proto`; generated TS in two `gen/` dirs |
| `tddy-screenshare` | `inspect.rs` (new), trait method, `lib.rs` |
| `tddy-vnc`, `tddy-rdp` | `--inspect` in `main.rs`; `inspect` impls |
| `tddy-host-service` | `PromptKind::DesktopCredentials` |
| `tddy-screen-sharing` | inspect RPC, remembered-credential path, answer decoding |
| `tddy-web` | `HostDesktopSignInDialog.tsx` (new), `HostDesktopOverlay.tsx`, page objects, `hostPromptFeed` kind |

## Implementation Milestones
- [ ] M1 Protos + inspect mode (bridges)
- [ ] M2 Daemon inspect + remembered path
- [ ] M3 Dialog + overlay flow

## Testing Plan

Bridge inspect: tddy-vnc against the node-3 fake RFB server; tddy-rdp against the node-2 NLA
server and a TLS-only server. Daemon: stand-in bridge script answering `--inspect`; a real
in-memory credential vault (as `session_vault_target_store_acceptance.rs` builds one) to prove
what is and is not written. Web: Cypress with `anInMemoryRpcBackend`, `aHostPromptFeed` emitting
`DESKTOP_CREDENTIALS`, and `anRsaOaepKeypair` to decrypt and assert the submitted answer.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `opens a desktop that needs no sign in without asking` | `packages/tddy-web/cypress/component/HostDesktopSignInAcceptance.cy.tsx` (new) | `NONE` path |
| `asks only for a password for vnc authentication` | same | field set |
| `asks for a username and password for apple remote desktop` | same | field set |
| `asks for a username domain and password for nla` | same | field set |
| `preselects the strongest method and lets the operator switch` | same | switcher |
| `sends remember with the credential when it is checked` | same | decrypted answer |
| `asks again with the desktops reason after a refused sign in` | same | re-ask |
| `says the desktop offers no sign in tddy supports` | same | no form |
| `inspecting_a_desktop_reports_the_methods_its_bridge_read` | `packages/tddy-screen-sharing` host-target module `tests` | inspect RPC |
| `a_remembered_credential_opens_the_desktop_without_asking` | same | no prompt |
| `a_credential_given_with_remember_is_sealed_in_the_operators_vault` | same | record written |
| `a_credential_given_without_remember_is_written_nowhere` | same | nothing on disk |
| `a_remembered_credential_the_desktop_refuses_is_not_used_again` | same | refused marking |
| `the_domain_the_operator_typed_reaches_the_bridge_on_stdin` | same | stdin JSON |
| `inspect_reports_the_security_types_a_vnc_server_advertises` | `packages/tddy-vnc/tests/vnc_inspect_integration.rs` (new) | VNC inspect |
| `inspect_reports_nla_for_a_server_that_requires_it` | `packages/tddy-rdp/tests/rdp_inspect_integration.rs` (new) | RDP inspect |

## Decisions & Trade-offs
- **On-demand handshake via the bridge's inspect mode** (developer, 2026-10-18) — keeps protocol knowledge in the bridge crates and the periodic probe byte-free.
- **Remember stores in the operator's credential vault** (developer, 2026-10-18), changing AC-SS-15 from "kept nowhere" to "kept nowhere unless Remember".

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

- [x] Record initial discovery (`2026-10-18-host-desktops-sign-in-form-initial-discovery.md`)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-host-desktops-sign-in-form-initial-discovery.md`
- [ ] USER REVIEW — work complete
