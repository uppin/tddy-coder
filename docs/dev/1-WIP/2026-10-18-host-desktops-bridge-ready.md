# Changeset: Report a desktop's sign-in failure and its real size

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**PR**: [#658](https://github.com/uppin/tddy-coder/pull/658)
**Type**: Feature + Bug fix
**Stack**: `#host-desktops` 6/8. Base: `feature/host-desktops/viewer-fullscreen`.

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

- [~] Testing — acceptance and unit tests written and failing (draft PR contract)

- [ ] Readiness protocol + `run_bridge` emission
- [ ] Authentication tagging in both bridges
- [ ] Daemon waits, maps, reaps; real size in replies
- [ ] Web shows start errors; resize TODO filed

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
- [ ] M1 Readiness types + bridge emission
- [ ] M2 Daemon wait/map/reap with stand-in bridges
- [ ] M3 Web error surface

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

## Technical Debt & Production Readiness

## Refactoring Needed
### From @red (TDD Red Phase)
- `tests/common/mod.rs` re-creates a minimal copy of the inline `mod tests` fixtures in `screen_sharing_service.rs` (stand-in bridge script, prompt-answering operator, in-memory session credential store, daemon builder). Once node 1 splits the file, fold the inline suite onto `tests/common/` (or a shared `testing` module) so there is one stand-in bridge.
- The inline suite's `FakeBridge` prints nothing on stdout: once `spawn_bridge` waits for readiness, every inline test that expects a start to succeed needs its stand-in to print a `ready` line (green must update it, or those tests hit the readiness timeout).
- `tddy-screen-sharing` gained `tddy-screenshare` as a **dev**-dependency (stand-ins print the real `BridgeReadiness` serialisation). Green moves it to `[dependencies]` when `await_bridge_readiness` parses with `read_readiness`.

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
