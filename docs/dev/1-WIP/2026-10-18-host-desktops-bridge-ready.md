# Changeset: Report a desktop's sign-in failure and its real size

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**PR**: [#658](https://github.com/uppin/tddy-coder/pull/658)
**Type**: Feature + Bug fix
**Stack**: `#host-desktops` 6/8. Base: `master` (repointed after #657 merged 2026-10-20; rebased `--onto origin/master` from the old base tip `3a2fab9fb`).

## Initial Discovery

[2026-10-18-host-desktops-bridge-ready-initial-discovery.md](./2026-10-18-host-desktops-bridge-ready-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-screen-sharing/docs/code-issues/oversized-file-screen-sharing-service.md` | ⚠ DURING | Node 1 splits the file; this node's daemon change lands in node 1's bridge-lifecycle module, not back in `screen_sharing_service.rs`. |
| [2026-10-18-screen-share-mid-session-resize.md](../todo/2026-10-18-screen-share-mid-session-resize.md) | (created here) | Deferred: the streamer refuses a frame size change. Out of scope; filed by this node. |

## Responsibility

- The **bridge readiness protocol**: `tddy_screenshare::readiness` — `BridgeReadiness` (`Ready { width, height }` / `Failed { kind: FailureKind, message }`, `FailureKind = Authentication | Connect | LiveKit`), written as one JSON line to the bridge's stdout by `run_bridge`.
- Authentication classification in both bridges: an error from `connect` that is a refused sign-in is tagged `Authentication` (`tddy_screenshare::AuthenticationRefused` marker error).
- Daemon: `spawn_bridge` (node 1's bridge-lifecycle module) reads the line with a bounded wait, returns the size, maps `Authentication` → `Unauthenticated`, other failures → `Unavailable`, timeout or early exit → `DeadlineExceeded` / `Unavailable`; the child is terminated on every failure. `StartStream` and `StartHostStream` return the reported size; `DEFAULT_STREAM_WIDTH/HEIGHT` are removed.
- Web: `HostDesktopOverlay` and the session target rows show the start error; the host overlay passes the reported size to input scaling (already wired to the reply's `width/height`).

## Boundaries

- Does not re-ask for credentials (node 8 consumes `Unauthenticated` for that).
- Does not handle a resize after the start (TODO filed).
- Does not change the prompt flow or the media gate.
- **2026-10-20 (widened):** negotiation's SecurityResult read for VNC Authentication moves into this PR by developer decision 2026-10-20, because vnc-rs reads that result itself and returns an untyped `VncError::General(reason)` on RFB 3.8 (`WrongPassword` on 3.7), so a refused password cannot be tagged `AuthenticationRefused` without it. The rest of negotiation (version, type choice, VeNCrypt, ARD, CredSSP) stays out of scope.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `service-split` | bridge-lifecycle module (`spawn_bridge`, `terminate_bridge`) | edits `spawn_bridge` there | re-split or move modules |
| n2 `rdp-nla` | `Credentials`; NLA refusal as a `connect` error | tags tddy-rdp's refusals `AuthenticationRefused` | change CredSSP |
| n3 `vnc-security` | `SecurityError::Refused` | maps it to `AuthenticationRefused` | change negotiation |

**Sequencing fact:** n1's module exists only once n1 is **green** (a restructure publishes nothing
earlier), so this node is wave 2.

## Draft PR contract

```rust
// packages/tddy-screenshare/src/readiness.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeReadiness { Ready { width: u32, height: u32 }, Failed { kind: FailureKind, message: String } }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind { Authentication, Connect, LiveKit }
pub fn write_readiness(out: &mut impl std::io::Write, readiness: &BridgeReadiness) -> std::io::Result<()>;
pub fn read_readiness(line: &str) -> Result<BridgeReadiness, serde_json::Error>;
#[derive(Debug, thiserror::Error)] #[error("the desktop refused the sign-in: {0}")]
pub struct AuthenticationRefused(pub String);
```
Published (node 1's split has not happened — developer decision 2026-10-18 — so the daemon seam is a
**new module**, not `screen_sharing_service.rs`):

```rust
// packages/tddy-screen-sharing/src/bridge_readiness.rs  (re-exported from lib.rs)
pub const DEFAULT_BRIDGE_READY_TIMEOUT: Duration;            // 30 s
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeStarted { pub width: u32, pub height: u32 }
pub async fn await_bridge_readiness<R: AsyncRead + Unpin + Send>(stdout: R, within: Duration)
    -> Result<BridgeStarted, Status>;                          // todo!()
// packages/tddy-screen-sharing/src/screen_sharing_service.rs (minimum to publish the config)
impl ScreenSharingServiceImpl {
    pub fn with_bridge_ready_timeout(self, timeout: Duration) -> Self; // real setter; field unread until green
}
```

Green: `spawn_bridge` pipes stdout, calls `await_bridge_readiness(child.stdout, self.bridge_ready_timeout)`,
terminates the child on every `Err`, and returns `BridgeStarted`; both start calls return its size.

Deviation from the drafted contract: `AuthenticationRefused` implements `Display`/`Error` by hand
instead of `#[derive(thiserror::Error)]` — `tddy-screenshare` has no `thiserror` dependency, and
adding one needs approval. Same message, same type.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** **no** — waits on node 1 being green (its module is where `spawn_bridge` lives after the split; node 1 itself waits on #626).
**Concurrent with:** node 7
**Blocks:** node 8 `sign-in-form` (re-ask on `Unauthenticated`)

    #626 → n1      n1 → n6, n7, n8      n2 ⇢ n3 (signature only)      n2, n3, n6 → n8

## Affected Packages

- `tddy-screenshare`, `tddy-vnc`, `tddy-rdp`, `tddy-screen-sharing`, `tddy-web`

## Related Feature Documentation

- PRD: `docs/ft/web/1-WIP/PRD-2026-10-18-host-desktops-bridge-ready.md`

## Summary

A wrong password says so; clicks land right on any desktop size.

## Scope

- [x] Testing — acceptance, unit and integration tests pass (scoped run, see Validation Results)

- [x] Readiness protocol + `run_bridge` emission
- [x] Authentication tagging in both bridges (VNC: `security::negotiate` owns the VNC Authentication SecurityResult read, `SecurityError::Refused` → `AuthenticationRefused`; RDP: a CredSSP connector error → `AuthenticationRefused`)
- [x] Daemon waits, maps, reaps; real size in replies
- [x] Web shows start errors; resize TODO filed

## Technical Changes

### State A
- `screen_sharing_service.rs:346-415` `spawn_bridge`: `.stdout(Stdio::null())`, returns after writing stdin.
- `:38-39` `DEFAULT_STREAM_WIDTH = 1920`, `DEFAULT_STREAM_HEIGHT = 1080`, returned at `:800-801`.
- `tddy-screenshare/src/bridge.rs:147` `wait_for_dimensions` knows the real size; nothing reports it.
- `HostDesktopOverlay.tsx:266-289` passes `stream.width/height` to `ScreenSharingOverlay` → `useScreenSharingInput` scaling.
- Stand-in shell bridge in service tests (`:1047-1105`) records argv + stdin.

### State B
Daemon reads one stdout line from the bridge within a bounded wait (configurable in tests).

### Delta
| Package | Change |
|---|---|
| `tddy-screenshare` | `readiness.rs` (new), `bridge.rs` emission, `lib.rs` |
| `tddy-vnc`, `tddy-rdp` | tag refusals |
| `tddy-screen-sharing` | bridge-lifecycle module; host and session start; tests |
| `tddy-web` | `HostDesktopOverlay.tsx`, session target row error; page objects |

## Implementation Milestones
- [x] M1 Readiness types + bridge emission
- [x] M2 Daemon wait/map/reap with stand-in bridges
- [x] M3 Web error surface

## Testing Plan

Daemon tests reuse the existing stand-in shell bridge, extended to print a scripted readiness line
(or none, or exit early). Bridge-side: `write_readiness`/`read_readiness` unit tests, and an
integration test in tddy-vnc that a refused password yields an `AuthenticationRefused`-tagged error.
Web: Cypress with the in-memory backend answering `StartHostStream` with `Code.Unauthenticated`.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_desktop_that_refuses_the_sign_in_fails_the_start_as_unauthenticated` | `packages/tddy-screen-sharing/tests/bridge_readiness_acceptance.rs` (placed outside the service file so node 1's split does not move them; fixtures in `tests/common/mod.rs`) | mapping + reason |
| `a_refused_start_leaves_no_bridge_running` | same | reaped |
| `a_started_desktop_reports_the_size_its_bridge_saw` | same | no 1920×1080 |
| `a_bridge_that_exits_before_reporting_fails_the_start` | same | early exit |
| `a_bridge_that_never_reports_fails_the_start_and_is_stopped` | same | timeout + terminate |
| `a_session_desktop_reports_its_real_size_too` | same | session scope |
| `a_refused_vnc_password_is_reported_as_an_authentication_failure` | `packages/tddy-vnc/tests/vnc_security_integration.rs` | tagging |
| `says the desktop refused the sign in instead of waiting` | `packages/tddy-web/cypress/component/HostDesktopStartFailureAcceptance.cy.tsx` (new) | host overlay error |
| `scales a click into the size the host reported` | same | 1280×800 aim |

Unit (`tddy-screenshare/src/readiness.rs`): `a_ready_line_round_trips`, `a_failure_line_names_its_kind`.

## Decisions & Trade-offs
- **stdout line, not a new channel** — the bridge's stdout is unused (`Stdio::null()`), JSON-on-stdin already sets the precedent, and it needs no new RPC between daemon and bridge.
- **Both scopes** — the bug and the silent failure are the same on the session scope.
- **VNC Authentication SecurityResult (2026-10-20, developer decision: own the read)** — tddy-vnc reads the SecurityResult in `security::negotiate`, so a refusal is the typed `SecurityError::Refused` on RFB 3.7 and 3.8. vnc-rs's DES (`client::security::des`) and its `AuthHelper` are private, so this crate has no DES primitive for the challenge/response. Done: `des` is a direct dependency of tddy-vnc (already in Cargo.lock at 0.9.0, via picky-krb) and write the response in `security`, then hand vnc-rs the RFB 3.3 None replay that the VeNCrypt/ARD path already uses. Rejected: copying vnc-rs's DES (a reimplementation), forking vnc-rs (larger change), observing vnc-rs's SecurityResult read on the wire (protocol-coupled). The new direct dependency (`des = "0.9"`, already locked) was approved by the developer 2026-10-20; `Cargo.lock` changes only by tddy-vnc gaining it.

## Technical Debt & Production Readiness

## Refactoring Needed
### From @red (TDD Red Phase)
- Deferred ([todo](../todo/2026-10-20-screen-sharing-two-stand-in-bridges.md)): `tests/common/mod.rs` re-creates a minimal copy of the inline `mod tests` fixtures in `screen_sharing_service.rs` (stand-in bridge script, prompt-answering operator, in-memory session credential store, daemon builder). Once node 1 splits the file, fold the inline suite onto `tests/common/` (or a shared `testing` module) so there is one stand-in bridge.
- ✅ (done in green) The inline suite's `FakeBridge` prints nothing on stdout: once `spawn_bridge` waits for readiness, every inline test that expects a start to succeed needs its stand-in to print a `ready` line (green must update it, or those tests hit the readiness timeout).
- ✅ (done in green) `tddy-screen-sharing` gained `tddy-screenshare` as a **dev**-dependency (stand-ins print the real `BridgeReadiness` serialisation). Green moves it to `[dependencies]` when `await_bridge_readiness` parses with `read_readiness`.

### From /validate-changes
### From /validate-tests
- Exact equality for the refusal reason in `bridge_readiness_acceptance.rs` and the tddy-vnc refusal test; name the `1280×800` desktop.
- Two stand-in bridges: deferred, see the todo entry above.
### From /validate-prod-ready
- None.
### From /analyze-clean-code
- `spawn_bridge` split into helpers (refactor subagent).
- `start_stream`, `start_bridge`: deferred, stack-shared files ([todo](../todo/2026-10-20-screen-sharing-start-functions-over-length.md)).

## Validation Results
### /validate-changes
**2026-10-20 — pass 1.** Risk: Critical 0, Warning 1, Info 3.

Stack gate: base #657 squash-merged into master and #658 repointed to `master`; rebased
`--onto origin/master 3a2fab9fb` (backup branch kept), range-diff identical, pushed with a lease
(`841b1edd3` → `97ea1d1c5`). Leak check clean: `origin/master..HEAD` is this PR's four commits, 26
files, all claimed by this changeset. Successor #659 (`x11-channels`) is now stale on its base.

Stack boundary: Responsibility delivered (no stubs in the touched `src/`); Dependencies not
re-implemented; the widened Boundary (VNC Authentication SecurityResult read, developer decision
2026-10-20) is recorded above and is the only negotiation change; no dependent's behaviour; no
parent-owned file deleted.

Build: `./dev cargo clippy -p tddy-vnc -p tddy-screenshare -p tddy-screen-sharing -p tddy-rdp --all-targets -- -D warnings` clean.
Tests: `./test -p tddy-screenshare -p tddy-vnc -p tddy-rdp -p tddy-screen-sharing`, 117 passed, 0 failed;
Cypress `HostDesktopStartFailureAcceptance.cy.tsx`, 3/3 passing.

| Item | Old | Actual | Now |
|---|---|---|---|
| Authentication tagging in both bridges | 🔲 | VNC via `security::negotiate` + `tag_refused_sign_in`; RDP CredSSP → `AuthenticationRefused` | ✅ |
| Testing | ⚠️ red | all pass | ✅ |
| — | — | RDP tagging test `a_wrong_password_on_an_nla_server_is_tagged_as_a_refused_sign_in` (verified failing with the tag removed) | 🆕 |
| — | — | Session-row start error Cypress test `says on the session's target row that the desktop refused the sign in` (verified failing with the row error hidden) | 🆕 |
| — | — | Session-scope `try_spawn_bridge`: a `prepare_bridge` failure is now `FailedPrecondition` instead of a silent `info!` skip | 🆕 |

Findings:
- **[WARNING]** `screen_sharing_service.rs` `start_stream`: no daemon config, an unknown target or an
  unconvertible target still answers `Ok` with `0×0` and no bridge. Pre-existing. AC-SS-9's
  `opening_a_desktop_returns_the_coordinates_a_viewer_needs` pins success without config, so this is
  out of scope here: filed as [2026-10-20-session-start-without-a-bridge-succeeds.md](../todo/2026-10-20-session-start-without-a-bridge-succeeds.md).
- **[INFO]** `BridgeSpawnConfig.width/height` are sent as `0`; nothing reads `BridgeConfig.width/height`
  (the bridges take the size from the server: `wait_for_dimensions`, RDP `desktop_size`).
- **[INFO]** The first stdout line of a bridge must be the readiness line: anything else a bridge
  printed first fails the start as "unreadable". Bridges log to stderr only, so this holds today.
- **[INFO]** Two stand-in bridges: the inline `FakeBridge` (`src/screen_sharing_service.rs` `mod tests`)
  and `tests/common/mod.rs` `StandInBridge`. An integration test crate cannot share a `#[cfg(test)]`
  module of the library, so merging them needs a shared testing module or feature: left to
  /validate-tests.
### /validate-tests
**2026-10-20.** Checked against fluent-tests (generic guidelines, `rust/std-test.md`,
`typescript/cypress-component.md`). Tests analyzed: 13 new or changed (6 daemon acceptance, 1 tddy-vnc,
1 tddy-rdp, 2 readiness unit, 3 Cypress) plus the inline `FakeBridge` change. Critical 0, Warning 3, Info 2.
No `#[ignore]`, `.only` or `.skip`; no branching in a test body; Given/When/Then throughout; Cypress
goes through page objects and `mountWithRpc`. Timeouts over budget (`A_SHORT_READINESS_WAIT` 5 s,
`A_START_FINISHES_WITHIN` 20 s) carry a justifying comment.

- **[WARNING]** `tests/bridge_readiness_acceptance.rs` `a_desktop_that_refuses_the_sign_in_fails_the_start_as_unauthenticated`:
  `refusal.message.contains(THE_DESKTOPS_REASON)` is a loose matcher; the daemon passes the bridge's
  message on verbatim, so assert equality.
- **[WARNING]** `packages/tddy-vnc/tests/vnc_security_integration.rs` `a_refused_vnc_password_is_reported_as_an_authentication_failure`:
  `refusal.0.contains(WRONG_PASSWORD_REASON)`, likewise (the reason is read verbatim off the wire).
- **[WARNING]** `tests/bridge_readiness_acceptance.rs`: `1280, 800` appears four times unnamed.
- **[INFO]** Two stand-in bridges (inline `FakeBridge` records argv and stdin; `tests/common`
  `StandInBridge` records a pid and scripts readiness). Merging is not mechanical: an integration-test
  crate cannot reach the library's `#[cfg(test)]` module, so it needs a shared testing module or
  feature, and the inline suite lives in `screen_sharing_service.rs`, which #659 and #660 also touch.
  Deferred: [2026-10-20-screen-sharing-two-stand-in-bridges.md](../todo/2026-10-20-screen-sharing-two-stand-in-bridges.md).
- **[INFO]** `common/mod.rs` `#![allow(dead_code)]` is the usual shared-`tests/common` allowance.
### /validate-prod-ready
**2026-10-20 — ✅ Ready.** 15 production files checked (test files, `tests/`, `cypress/` excluded).
Blockers 0, Warnings 0.

| Category | Count | Status |
|---|---|---|
| Mock code | 0 | ✅ |
| Dev fallbacks | 0 | ✅ (the old 1920×1080 guess is removed; the no-bridge `0×0` is filed as a todo, not a fallback) |
| TODO/FIXME added | 0 | ✅ (the `TODO(restructure)` markers in `screen_sharing_service*.rs` are n1's, with their todo link) |
| Unused code | 0 | ✅ |
| Debug output | 0 | ✅ (the bridge's stdout is the readiness line only; logging stays on stderr) |

Acceptable: `packages/tddy-vnc/src/security.rs` `vnc_auth_response` `.expect("a chunk of the DES block length is a DES block")`
is infallible (`chunks_exact_mut(8)` of a 16-byte array).
### /analyze-clean-code
**2026-10-20 — score C before cleanup** (2 must-refactor that this PR grew, 1 pre-existing).
15 non-test source files; none over 500 lines of production code (file-length gate below).

| Function | Lines (base → now) | Verdict |
|---|---|---|
| `bridge_lifecycle.rs` `spawn_bridge` | 70 → 102 | must refactor → split into helpers in place (file not touched by #659/#660) |
| `screen_sharing_service.rs` `start_stream` | 63 → 72 | must refactor → **deferred**, stack-shared file ([todo](../todo/2026-10-20-screen-sharing-start-functions-over-length.md)) |
| `host_targets.rs` `start_host_stream` | 67 → 67 | pre-existing, not grown |
| `tddy-screenshare` `bridge.rs` `start_bridge` / `run_bridge` | — → 58 / 68 → 56 | needs attention; stack-shared, deferred (same todo) |
| `bridge_readiness.rs` `await_bridge_readiness` | new, 33 | acceptable |
| `security.rs` `sign_in_vnc_authentication` / `vnc_auth_response` | new, 24 / 20 | acceptable / excellent |
| `bridge.rs` `classify` | new, 18 | excellent |

Parameters: `try_spawn_bridge` keeps its pre-existing 8 parameters (`#[allow(clippy::too_many_arguments)]`, not
grown here). Nesting ≤ 3 in every new function. Magic values: none unnamed (`DEFAULT_BRIDGE_READY_TIMEOUT`,
`VNC_AUTH_CHALLENGE_LEN`, `VNC_AUTH_KEY_LEN`; the `0×0` no-bridge size is commented). Duplication: the two
stand-in bridges (see /validate-tests). Naming: consistent with the modules' prose style.
Minor: `security.rs` module doc line 17 runs past 100 columns (rustfmt leaves comments alone), and #659 and
#660 rewrite this file, so it is left alone.

**File-length gate (3.5):** production lines base → now, none ≥ 500. `screen_sharing_service.rs` 421 → 439,
`bridge.rs` 301 → 397, `security.rs` ≈ 449 (production lines up to `mod tests`), `vnc_client.rs` 336 → 349,
`rdp_client.rs` 357 → 360, `bridge_lifecycle.rs` 243 → 272, `SessionScreenSharingTab.tsx` 337 → 356; new
files `bridge_readiness.rs` 74, `readiness.rs` 68, `screenSharingStartFailure.ts` 20. No decomposition needed.

## TODO

- [x] Record initial discovery (`2026-10-18-host-desktops-bridge-ready-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail) — 6/6 daemon, 1/1 tddy-vnc, 2/2 readiness unit fail; Cypress: 1 fails, `scales a click into the size the host reported` **already passes** (the overlay already scales into the reply's `width/height`) and is kept as a guard
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-host-desktops-bridge-ready-initial-discovery.md`
- [ ] USER REVIEW — work complete
