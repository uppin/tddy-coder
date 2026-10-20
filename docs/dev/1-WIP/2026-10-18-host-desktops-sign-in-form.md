# Changeset: Desktop sign-in shaped by its advertised methods, with Remember

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**PR**: [#660](https://github.com/uppin/tddy-coder/pull/660)
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

**Sequencing facts (recorded with the failing tests):**

- `vnc_inspect_integration.rs` fails today on this node's `todo!()`, but its green implementation runs
  node 3's `parse_security_types`, which is still `todo!()` — **n3 must be green first**.
- `a_remembered_credential_the_desktop_refuses_is_not_used_again` fails today at its *Given* (the
  refused open must come back `Unauthenticated`), which is node 6's readiness wait — **n6 must be
  green first**. Every other daemon sign-in test also reaches `StartHostStream`'s readiness wait
  once n6 is green, so the stand-in bridge reports `ready` after reading its config.
- `the_domain_the_operator_typed_reaches_the_bridge_on_stdin` needs a `domain` on the daemon's
  bridge config, which node 2's `BridgeConfig.domain` reads — the bridge half is n2's; this node adds
  the daemon half.
- Module placement: the sign-in surface is `tddy-screen-sharing/src/host_desktop_sign_in.rs`; the
  service file gained only the one-line `inspect_host_desktop` delegator, so n1's split moves nothing
  of this node's.

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

**As published (red phase, 2026-10-18)** — where the real code differed from the sketch above:

- `HostPromptEvent.offered_auth_methods` is field **8**, not 9: 8 was the next free number and
  nothing reserves it.
- `domain` is `string domain = 8` on `ScreenSharingTarget` (after node 7's `x11_display = 7`) and
  `string domain = 9` on `AddHostTargetRequest` (after its `x11_display = 8`). Rust literals fill
  `String::new()`; nothing persists it yet (`HostDesktopTarget` has no `domain`).
- `HostDesktopSignInDialog`'s `question` is `HostPromptEventLike`, not `HostPromptEvent`: the
  overlay latches the feed's `HostPromptEventLike`, and requiring the wire message would force a
  cast at the only call site.
- Rust surface (bodies `todo!()` unless marked real):
  ```rust
  // tddy-screenshare/src/inspect.rs (new)
  pub struct InspectTarget { pub host: String, pub port: u16 }                 // serde, real
  pub enum AdvertisedMethod { None, VncPassword, VencryptPlain, VencryptTlsPlain,
      AppleRemoteDesktop, RdpTls, RdpNla, UnsupportedVnc { security_type: u8 } } // serde snake_case, real
  pub struct InspectReport { pub methods: Vec<AdvertisedMethod> }              // serde, real
  pub enum BridgeMode { Bridge, Inspect }  pub const INSPECT_FLAG: &str = "--inspect";
  impl BridgeMode { pub fn from_args(args: impl Iterator<Item = String>) -> anyhow::Result<Self> } // real
  pub fn write_inspect_report(out: &mut impl Write, report: &InspectReport) -> io::Result<()>;
  pub fn read_inspect_report(line: &str) -> Result<InspectReport, serde_json::Error>;
  pub async fn run_inspect<C: ScreenSharingClient>(target: InspectTarget) -> anyhow::Result<()>;
  // ScreenSharingClient
  async fn inspect(host: &str, port: u16) -> anyhow::Result<Vec<AdvertisedMethod>>;
  // tddy-vnc / tddy-rdp main.rs: `BridgeMode::from_args` → run_bridge | run_inspect (real)
  // tddy-host-service
  PromptKind::DesktopCredentials  → HostPromptKind::DesktopCredentials           // real
  // tddy-screen-sharing/src/host_desktop_sign_in.rs (new)
  pub async fn inspect_host_desktop(&ScreenSharingServiceImpl, Request<InspectHostDesktopRequest>)
      -> Result<Response<InspectHostDesktopResponse>, Status>;
  pub fn decrypt_desktop_credential_answer(&dyn HostKeypair, &[u8]) -> Result<DesktopCredentialAnswer, Status>;
  pub fn remembered_account(daemon_instance_id: &str, target_id: &str) -> AccountId;
  pub struct RememberedDesktopCredentials { .. }  // new(Arc<SessionVaults>), usable, remember, mark_refused
  ```
- The stand-in bridge's inspect line is `serde_json` of `InspectReport`, e.g.
  `{"methods":["apple_remote_desktop",{"unsupported_vnc":{"security_type":16}}]}`.

**Open for green:** a `NONE` desktop opens "without asking" — either the daemon does not prompt for
a target whose inspection said `NONE` (inspection cached per target), or the overlay answers the
prompt itself without a form. The web test pins only the outcome (no form, no answer sent, the
desktop opens after an inspection); the daemon tests do not pin either choice.

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

- [x] Testing — acceptance and unit tests written (red 2026-10-18) and green
- [x] Protos + generated TS
- [x] Inspect mode in both bridges (`tddy-vnc` `security::inspect`, `tddy-rdp` `RdpClient::inspect`)
- [x] Daemon inspect, remembered credential, prompt with method, remember write, refused marking
- [x] Sign-in dialog; overlay inspect → form → start → re-ask
- [x] 🆕 Inspect line types moved to `tddy_daemon_extensions::desktop_inspect` after the rebase onto node 6's move of the readiness protocol (`tddy-screen-sharing` no longer depends on `tddy-screenshare`, `livekit_is_a_satellite`); `tddy-screenshare::inspect` keeps `BridgeMode` and `run_inspect` and re-exports the rest
- [x] 🆕 `HostKeyContinuityNotices.tsx` extracted from `HostPassphraseDialog` so both dialogs render one key-continuity notice set

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
- [x] M1 Protos + inspect mode (bridges)
- [x] M2 Daemon inspect + remembered path
- [x] M3 Dialog + overlay flow

## Testing Plan

Bridge inspect: tddy-vnc against the node-3 fake RFB server; tddy-rdp against the node-2 NLA
server and a TLS-only server. Daemon: stand-in bridge script answering `--inspect`; a real
in-memory credential vault (as `session_vault_target_store_acceptance.rs` builds one) to prove
what is and is not written. Web: Cypress with `anInMemoryRpcBackend`, `aHostPromptFeed` emitting
`DESKTOP_CREDENTIALS`, and `anRsaOaepKeypair` to decrypt and assert the submitted answer.

## Acceptance Tests

Daemon tests are placed in `packages/tddy-screen-sharing/tests/`, outside the service file, so node
1's split does not move them; they extend node 6/7's `tests/common/mod.rs`.

| Test | File | Validates | Fails today because |
|---|---|---|---|
| [x] `opens a desktop that needs no sign in without asking` | `packages/tddy-web/cypress/component/HostDesktopSignInAcceptance.cy.tsx` (new) | `NONE` path: inspected first, no form, no answer, opens | the overlay never calls `InspectHostDesktop` |
| [x] `asks only for a password for vnc authentication` | same | field set | no sign-in form is mounted |
| [x] `asks for a username and password for apple remote desktop` | same | field set | no sign-in form is mounted |
| [x] `asks for a username domain and password for nla` | same | field set | no sign-in form is mounted |
| [x] `preselects the strongest method and lets the operator switch` | same | switcher; decrypted answer's method | no sign-in form is mounted |
| [x] `sends remember with the credential when it is checked` | same | decrypted `DesktopCredentialAnswer` | no sign-in form is mounted |
| [x] `asks again with the desktops reason after a refused sign in` | same | re-ask with reason | no sign-in form is mounted |
| [x] `says the desktop offers no sign in tddy supports` | same | no form, names the types, no start | the overlay says "Connecting…" |
| [x] `inspecting_a_desktop_reports_the_methods_its_bridge_read` | `packages/tddy-screen-sharing/tests/host_desktop_sign_in_acceptance.rs` (new) | inspect RPC | `todo!()` in `host_desktop_sign_in::inspect_host_desktop` |
| [x] `inspecting_a_desktop_tells_its_bridge_only_where_the_desktop_is` (added) | same | `{host, port}` on the inspect stdin | `todo!()` |
| [x] `inspecting_a_desktop_with_a_remembered_credential_says_so` (added) | same | `remembered` | `todo!()` |
| [x] `opening_a_desktop_asks_the_operator_for_its_credentials` (added) | same | prompt kind `DesktopCredentials` | asks `DesktopPassword` |
| [x] `a_remembered_credential_opens_the_desktop_without_asking` | same | no prompt on reopen | asked twice |
| [x] `a_credential_given_with_remember_is_sealed_in_the_operators_vault` | same | record `screen-sharing` / `host:<instance>:<target>` | no record |
| [x] `a_credential_given_without_remember_is_written_nowhere` | same | no record, no file holds it | the bridge got the encoded answer as its password |
| [x] `a_remembered_credential_the_desktop_refuses_is_not_used_again` | same | refused marking | its Given needs n6's `Unauthenticated` (start succeeds) |
| [x] `the_domain_the_operator_typed_reaches_the_bridge_on_stdin` | same | stdin JSON `username`/`domain`/`password` | no `domain`; answer not decoded |
| [x] `inspect_reports_the_security_types_a_vnc_server_advertises` | `packages/tddy-vnc/tests/vnc_inspect_integration.rs` (new) | VNC inspect | `todo!()` in `VncClientState::inspect` |
| [x] `inspect_reports_no_authentication_for_a_server_that_asks_for_none` (added) | same | `None` | `todo!()` |
| [x] `inspect_reports_the_vencrypt_subtypes_a_vnc_server_offers` (added) | same | VeNCrypt subtypes | `todo!()` |
| [x] `inspect_selects_no_sign_in_method` (added) | same | never starts a sign-in | `todo!()` |
| [x] `inspect_reports_nla_for_a_server_that_requires_it` | `packages/tddy-rdp/tests/rdp_inspect_integration.rs` (new) | RDP inspect | `todo!()` in `RdpClient::inspect` |
| [x] `inspect_reports_tls_for_a_server_that_offers_only_tls` (added) | same | RDP TLS | `todo!()` |
| [x] `an_inspect_report_is_one_line_that_reads_back_as_written` (unit) | `packages/tddy-screenshare/src/inspect.rs` | report line round-trip | `todo!()` |
| [x] `an_inspect_line_names_each_method_in_snake_case` (unit) | same | wire spelling | `todo!()` |
| `a_bridge_given_no_arguments_bridges`, `…_the_inspect_flag_inspects`, `…_any_other_argument_refuses_it_by_name` (unit) | same | `--inspect` parsing | pass — the parsing is real |

Unchanged and still green: `HostDesktopConnectAcceptance.cy.tsx` (13), `HostAddKeyAcceptance.cy.tsx`
(15). **Green will have to update `HostDesktopConnectAcceptance`'s backends** once the overlay
inspects first and answers `DESKTOP_CREDENTIALS` instead of `DESKTOP_PASSWORD`: they implement no
`inspectHostDesktop` and raise `DESKTOP_PASSWORD` questions answered through `HostPassphraseDialog`.

## Decisions & Trade-offs
- **On-demand handshake via the bridge's inspect mode** (developer, 2026-10-18) — keeps protocol knowledge in the bridge crates and the periodic probe byte-free.
- **Remember stores in the operator's credential vault** (developer, 2026-10-18), changing AC-SS-15 from "kept nowhere" to "kept nowhere unless Remember".

## Technical Debt & Production Readiness

- [x] Fingerprint derivation in the sign-in dialog — deferred: [2026-10-21-desktop-sign-in-pins-the-advertised-fingerprint.md](../todo/2026-10-21-desktop-sign-in-pins-the-advertised-fingerprint.md)
- [x] `HostPromptEvent.offered_auth_methods` unused — deferred: [2026-10-21-host-prompt-offered-auth-methods-is-never-sent.md](../todo/2026-10-21-host-prompt-offered-auth-methods-is-never-sent.md)

## Refactoring Needed
### From @red (TDD Red Phase)
- `tests/common/mod.rs` (screen-sharing) grows a stand-in that answers `--inspect`, a re-scriptable
  bridge (`StandInBridge::now_does`), a configs log, a generalised operator (`TheOperator`, replacing
  `AnOperatorWhoTypesNoPassword`) and the operator's real vault (`with_the_operators_vault`). It still
  duplicates the inline suite's fixtures (node 6/7's note) — fold together once node 1 splits the
  service file. Stand-ins now live in a `tools` tempdir apart from the daemon's storage, so "written
  nowhere" can search the daemon's files without finding the stand-in's own records.
- `rdp_inspect_integration.rs` copies the TLS-identity and `ironrdp-server` desktop setup from
  `rdp_nla_integration.rs`; `vnc_inspect_integration.rs` has its own advertising-only RFB server next
  to `vnc_security_integration.rs`'s full one. Extract a shared fake RDP desktop and RFB handshake into
  a `tests/support` module per crate.
- `host_desktop_sign_in::inspect_host_desktop` takes `&ScreenSharingServiceImpl`, whose fields are
  private to `screen_sharing_service.rs`: green needs `pub(crate)` accessors (host scope, config, the
  remembered-credential store) — or node 1's split, which makes those modules siblings anyway.
- `with_credential_vaults` turns the vaults into a `SessionVaultTargetStore` and keeps nothing else;
  green has to keep a `RememberedDesktopCredentials` beside it from the same `Arc<SessionVaults>`.
- `host_prompt_stream.rs` sends `offered_auth_methods: Vec::new()` (TODO): `PendingPrompt` carries no
  methods. Either `PendingPrompt` grows them or the field is dropped in favour of `InspectHostDesktop`.
- `BridgeConfig` has `domain` (n2) but the daemon's `BridgeSpawnConfig` does not; add it.
- Node 6's web test `says the desktop refused the sign in instead of waiting` asserts the overlay's
  status line; after this node the overlay re-asks instead — keep the status, or move that test's
  assertion onto `hostDesktopSignInPage.error()`.
- `prost = "0.13"` added as a **dev**-dependency of `tddy-screen-sharing` (already in the lockfile
  via `tddy-service`); the green decode of the answer will need it as a normal dependency.
### From /validate-changes
- ✅ Fixed (back to /green once): a credential typed with Remember was sealed in the vault *before* the bridge started, so a wrong password was stored and then marked refused. Now written only after a successful start; an already-remembered credential the desktop refuses is still marked refused.
- ✅ Fixed (back to /green once): a **locked** operator vault failed every host-desktop open (`usable` mapped `Locked` to an error). A locked vault now means nothing usable is remembered: the operator is asked.
- ✅ Fixed (back to /green once): VNC inspect dropped every type a server advertised after VeNCrypt (`break` after committing to type 19). VeNCrypt's subtypes are now appended after the other types.
- Minor, for /pr-wrap: `HostPassphraseDialog.allowEmpty` has no caller left; `PromptKind::DesktopPassword` is raised by nothing (the proto value stays for wire compatibility); `HostKeyContinuityNotices` fragment is over-indented.
### From /validate-tests
- [WARNING] `host_desktop_sign_in_acceptance.rs`: `inspect_host_desktop`'s X11 branch (`target.display.is_some()` → `NONE`, no bridge run) has no test. Add one.
- [WARNING] `host_desktop_sign_in_acceptance.rs`: no test for an inspection whose bridge fails (non-zero exit → `Unavailable` with its stderr). Add one if the stand-in can be scripted to fail inspect.
- [DECISION] `HostDesktopConnectAcceptance.cy.tsx` — the approved `says the answer may be left blank when the desktop has no password` is **removed, not restored**: the blank-answer prompt it pinned no longer exists. A desktop advertising `NONE` gets no form at all — the overlay answers the host's question itself — so there is no dialog to say "blank" in. Its intent (an operator is never made to invent a password a desktop does not have) is now pinned by the same spec's `opens a desktop that has no password without asking the operator` and by `HostDesktopSignInAcceptance.cy.tsx`'s `opens a desktop that needs no sign in without asking`. Recorded in State B and *Decisions*.
### From /validate-prod-ready
- [TODO] `HostDesktopSignInDialog.tsx` `TODO(host-desktops)` (pins the advertised fingerprint, `checkHostKey`, rather than one derived from the key, `verifyHostKey`): **deferred** — deriving it makes every question in two approved specs `mismatched`, because they publish a generated key beside a fixed fake fingerprint, so the fix is a spec-fixture change. Recorded in [2026-10-21-desktop-sign-in-pins-the-advertised-fingerprint.md](../todo/2026-10-21-desktop-sign-in-pins-the-advertised-fingerprint.md); the TODO comment links it. The password dialog had the same check before this PR.
- [UNUSED] `HostPromptEvent.offered_auth_methods` is always sent empty, and its proto comment claims otherwise. **Correct the comment** now; the field's removal is deferred in [2026-10-21-host-prompt-offered-auth-methods-is-never-sent.md](../todo/2026-10-21-host-prompt-offered-auth-methods-is-never-sent.md).
- [DECISION] When the vault write after a successful start fails (G1), the error is logged with `log::error!` (naming the desktop, never the secret), and the started stream is returned rather than torn down. The next open asks again. This is deliberate: a working sign-in is not thrown away over a storage error. The vault was already checked to be open before the start, so this path is reached only if the vault closes during the start.
### From /analyze-clean-code

## Validation Results
### /validate-changes
**2026-10-20 ([W] stage).** Stack gate: base `feature/host-desktops/x11-channels` (PR #659, resolved from the PR's `baseRefName`); rebased `--onto origin/feature/host-desktops/x11-channels 382db6473` (n7's old tip); leak check `origin/<base>..HEAD` = this PR's own commits only; no paths deleted against the base. Conflicts: `tests/common/mod.rs` (imports: union, readiness from `tddy_daemon_extensions::bridge_readiness`), `tddy-screenshare/src/lib.rs` (union), `screen_sharing_service/host_targets.rs` (n7's `serve_x11_display_to_the_bridge` / `spawn_host_bridge` kept, the refused-marking wrapped around `spawn_host_bridge`), `HostDesktopOverlay.tsx` (union of `isTargetOfChannel` and `noSupportedSignInText`). `grep -rn tddy_screenshare::readiness` is empty. `Cargo.lock` gains only the `prost 0.13.5` edge of `tddy-screen-sharing` — no new crate version.

| Check | Result |
|---|---|
| Changeset items implemented or deferred | ✅ all |
| `## Responsibility` delivered | ✅ no `todo!()` / `TODO(sign-in-form)` left in the touched crates |
| `## Dependencies` not implemented here | ✅ |
| `## Boundaries` respected | ✅ probe unchanged, session targets keep their form |
| Diff only this PR's files | ✅ |
| Parent-owned files intact | ✅ |

Builds: `cargo check --all-targets` for `tddy-daemon-extensions`, `tddy-screenshare`, `tddy-screen-sharing`, `tddy-vnc`, `tddy-rdp` ✅. Pre-fix `./test -p tddy-screen-sharing`: 73 passed, 0 failed.

Findings: 3 warnings fixed through /green (above, *From /validate-changes*); 3 minor cleanups for /pr-wrap. Decisions recorded: the **"Open for green"** choice is the overlay answering itself — the daemon still raises `DESKTOP_CREDENTIALS` on every start (an X11 channel and a `NONE` desktop included; `InspectHostDesktop` answers `NONE` for an X11 display without running a bridge), and the overlay answers a `NONE` desktop's question with an encrypted `{method: NONE}` and no form. Node 7's x11vnc step stays in the host-target start path, between the credential and the bridge spawn. `HostPromptEvent.offered_auth_methods` is sent empty: the methods come from `InspectHostDesktop`, not the prompt.
### /validate-tests
**2026-10-21.** Analyzed: 13 daemon sign-in acceptance tests, 5 VNC inspect, 2 RDP inspect, 5 screenshare/extension-port unit tests, 8 web sign-in specs, the 12 updated connect specs and the touched start-failure / channels / input-scope specs. Fluent-tests: Given/When/Then throughout, builders (`a_credential()…`, `a_daemon_whose_bridge(…)`), page objects (`hostDesktopSignInPage`), decrypted-and-decoded assertions on what the host reads. No `#[ignore]`, `.only`, `.skip`; the one poll (`what_the_client_did`, 5 ms) is bounded by `AN_INSPECTION_FINISHES_WITHIN`. No description/body mismatches. Critical: 0. Warnings: 2 (coverage, above). One approved test removed as superseded, with its reason recorded (above).
### /validate-prod-ready
**2026-10-21 — ⚠️ gaps tracked.** No mock or fake code in production paths, no `println!`/`eprintln!`/`dbg!`/`console.log`, and no environment-conditional code. `expect()` calls appear only in inline tests. TODO markers: 1 added (deferred with a todo entry, above). Unused: `allowEmpty` and `PromptKind::DesktopPassword` were removed in step 1's refactor; `offered_auth_methods` is deferred. Blockers: 0.
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-18-host-desktops-sign-in-form-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-host-desktops-sign-in-form-initial-discovery.md`
- [ ] USER REVIEW — work complete
