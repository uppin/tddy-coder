# Changeset: Terminal exposure through an extension port

**Date**: 2026-10-16
**Status**: 🚧 In Progress
**PR**: #628
**Type**: Refactor (architecture)
**Stack**: #satellite 6/13. Base: `feature/satellite/core-modules` (#627).

## Initial Discovery

[2026-10-16-livekit-satellite-terminal-bridge-initial-discovery.md](./2026-10-16-livekit-satellite-terminal-bridge-initial-discovery.md)

## Responsibility

Exposing a PTY to remote participants as a port. Owns `TerminalExposure`, `TerminalAddress`,
`ExposedTerminal` and `NoTerminalExposure` in `tddy-daemon-extensions`, the plugin's
`LiveKitTerminalExposure`, `CliSessionManager::ensure_exposed_terminal`, the public
`pty_terminal_service`, and the switch of lifecycle's bridge and telegram's immediate bridge. Strikes
`tddy-cli-sessions`.

## Boundaries

- Does not change *when* a terminal is exposed. Node 1's `SessionReach` decides that.
- Does not shorten the lock spanning the join.
- Does not touch `tddy-terminal-rpc`'s `pty-relay` client side (node 7).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #623 `satellite-1-extension-ports` | `tddy-daemon-extensions`; `ReachableSessions::expose_terminal` (implemented by the host over `SessionTerminalBridge`); ratchet | Adds `terminal_exposure.rs`; the host's bridge calls the new port; strikes one entry | Change `ReachableSessions` or `SessionReach` |
| #624–#627 | Nothing consumed. The plugin mints the participant token itself, so there is **no** edge to #626's `RoomTokenMinter` (that edge would push this node into wave 3) | — | Use `RoomTokenMinter` |

**As published (wave 2):** port + null object + `ExposedTerminal` (real: a JoinHandle wrapper), `ensure_exposed_terminal` and `pty_terminal_service` (todo!()), the plugin's `LiveKitTerminalExposure` (todo!()), and the failing tests. Lifecycle's bridge and telegram's immediate bridge switch in green; the ratchet assertions wait on #623's `WorkspaceGraph`.

## Draft PR contract

```rust
// tddy-daemon-extensions/src/terminal_exposure.rs
pub struct TerminalAddress { pub room: String, pub identity: String }
/// A running exposure: ends when the room or the terminal does.
pub struct ExposedTerminal { /* JoinHandle<()> */ }
impl ExposedTerminal {
    pub fn new(serving: tokio::task::JoinHandle<()>) -> Self;
    pub fn is_finished(&self) -> bool;
}
#[async_trait::async_trait]
pub trait TerminalExposure: Send + Sync {
    /// Serve `terminal` under `at.identity` in `at.room`, advertising `metadata_json`.
    async fn expose(&self, at: &TerminalAddress, terminal: Arc<dyn tddy_rpc::RpcService>,
                    metadata_json: Option<String>) -> Result<ExposedTerminal, Status>;
}
/// Nothing to expose through: a failed precondition, which callers treat as "not exposed".
pub struct NoTerminalExposure;

// tddy-cli-sessions
pub fn pty_terminal_service(handle: Arc<PtyHandle>) -> Arc<dyn tddy_rpc::RpcService>;
impl CliSessionManager {
    pub async fn ensure_exposed_terminal(&self, session_id: &str, at: &TerminalAddress,
                                         exposure: &dyn TerminalExposure) -> Result<bool, Status>;
}

// tddy-daemon-livekit/src/terminal_exposure.rs
pub struct LiveKitTerminalExposure { .. }
impl LiveKitTerminalExposure { pub fn from_config(config: &DaemonConfig) -> Option<Arc<Self>>; }
impl TerminalExposure for LiveKitTerminalExposure { .. }
```

Bodies are `todo!()` until green; the failing tests below are published with the surface.

## Green wave

**Wave:** 2 of 5
**Greenable independently:** yes. The manager is tested against an in-memory exposure, and the
plugin against the existing testkit suites.
**Concurrent with:** nodes 2–5 and 7–9
**Blocks:** none

    n1 → n2..n9   n2, n4 → n10   n2, n3, n4, n7, n10 → n11   n4, n10 → n12   n11, n12 → n13

## Affected Packages

`tddy-daemon-extensions`, `tddy-cli-sessions`, `tddy-daemon-livekit`, `tddy-session-lifecycle`
(`terminal_bridge_impl.rs`, host builder), `tddy-telegram-control` (`session_start.rs`),
`tddy-daemon` (wiring).

## Summary

Exposing a terminal over LiveKit stops requiring the session manager to know LiveKit.

## Scope

- [ ] Port, address, handle, null object
- [ ] `pty_terminal_service` public and LiveKit-free (`tddy_rpc` types only)
- [ ] `LiveKitTerminalExposure` in the plugin (today's `spawn_livekit_bridge` body)
- [ ] `ensure_exposed_terminal`; `LiveKitTerminalAddress` removed
- [ ] Lifecycle bridge + telegram immediate bridge on the port; daemon wiring
- [ ] Ratchet: strike `tddy-cli-sessions`

## Technical Changes

### State A
- `tddy-cli-sessions/src/cli_session_manager/livekit_bridge.rs` (230 lines): `PtyLiveKitService`
  (`:27`, `impl RpcService` using `tddy_livekit::{RpcService, RpcResult}` re-exports) and
  `spawn_livekit_bridge(handle, url, room, api_key, api_secret, identity, metadata)` (`:153`). That
  function mints with `TokenGenerator` (86400 s), joins with `LiveKitParticipant::connect`, and
  republishes metadata through `spawn_local_participant_metadata_watcher` on
  `SESSION_METADATA_REPUBLISH_INTERVAL`.
- `livekit_terminals.rs` (99 lines): `expose_terminal_to_livekit` (`:35`) and
  `ensure_livekit_terminal` (`:61`, write lock held across the join for single-flight).
- `cli_session_manager.rs:71` `BridgedTerminal { metadata, participant: Option<JoinHandle> }`, `:85`
  `LiveKitTerminalAddress { url, room, api_key, api_secret, identity }`.
- Callers: `tddy-session-lifecycle/src/connection_service/terminal_bridge_impl.rs:15-57` (builds the
  address from `spawner::livekit_creds_from_config`); `tddy-telegram-control/.../session_start.rs:480-520`
  (calls `spawn_livekit_bridge` directly); `tddy-agent-launch/src/claude_cli_spawn.rs:274`
  (`expose_terminal_to_livekit`, metadata only).
- `tddy-cli-sessions` has no `tests/` directory and no `docs/code-issues/`.

### State B
The manager keeps the per-session record and the single-flight. It asks `exposure.expose(at,
pty_terminal_service(handle), metadata_json)` and stores the `ExposedTerminal`. The plugin's
`LiveKitTerminalExposure` reads URL and credentials from config and does the rest.

### Delta
| Package | Change |
|---|---|
| `tddy-daemon-extensions` | `src/terminal_exposure.rs`, `lib.rs`; `tokio` `rt` (JoinHandle) |
| `tddy-cli-sessions` | `livekit_bridge.rs` → `pty_terminal_service.rs`; `livekit_terminals.rs` → `exposed_terminals.rs`; `cli_session_manager.rs`; `Cargo.toml` −`tddy-livekit` +`tddy-daemon-extensions` |
| `tddy-daemon-livekit` | `src/terminal_exposure.rs` (new), `lib.rs` |
| `tddy-session-lifecycle` | `terminal_bridge_impl.rs`, host holds `Arc<dyn TerminalExposure>` (`with_terminal_exposure`) |
| `tddy-telegram-control` | `session_start.rs` |
| `tddy-daemon` | `runtime.rs` wiring |
| ratchet | strike `tddy-cli-sessions` |

## Implementation Milestones
- [ ] M1 Port and null object
- [ ] M2 Manager on the port; in-memory exposure tests green
- [ ] M3 Plugin exposure; terminal-over-LiveKit suites green
- [ ] M4 Callers switched; manifest trimmed; ratchet green

## Testing Plan

The manager's single-flight and re-exposure logic is tested against an in-memory `TerminalExposure`
that records calls and returns controllable handles; that test needs no LiveKit. The real exposure
keeps its coverage through node 1's and the lifecycle's testkit-backed terminal-driving tests,
which must stay green.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_recorded_terminal_is_exposed_once_however_many_reaches_arrive_together` | `packages/tddy-cli-sessions/tests/exposed_terminals.rs` (new) | Concurrent `ensure_exposed_terminal` → one `expose` call; both answer `true` |
| `a_terminal_whose_exposure_ended_is_exposed_again` | same | A finished `ExposedTerminal` → a second `expose` |
| `a_session_that_recorded_no_terminal_is_not_exposed` | same | No record → `false`, no `expose` call |
| `the_exposure_advertises_the_session_metadata_block_at_the_given_address` | same | `metadata_json` passed = `session_metadata_json(block)` |
| `only_plugins_wiring_and_known_offenders_name_a_livekit_crate` + `every_known_offender_still_names_a_livekit_crate` | `packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` | Entry struck; both ratchet tests pass |

Unit (`tddy-daemon-extensions/src/terminal_exposure.rs`): `with_nothing_wired_exposing_is_a_failed_precondition`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **The port takes an `RpcService`, not a PTY.** The PTY type belongs to `tddy-cli-sessions`, and an
  `RpcService` is what any transport needs to serve a terminal.
- **Credentials leave the address.** Only the plugin needs them, so `TerminalAddress` is room and
  identity.
- **No edge to node 4.** Minting through `RoomTokenMinter` would be tidier, but it would make this
  node depend on #626's behaviour and step the line back a wave. The plugin's exposure mints with
  `TokenGenerator` directly, which the plugin crate may name.

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

- [x] Record initial discovery (`2026-10-16-livekit-satellite-terminal-bridge-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: developer asked to finish the stack without stopping, 2026-10-16)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete
