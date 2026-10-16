# Changeset: tddy-coder's room participant through a port

**Date**: 2026-10-16
**Status**: 🚧 In Progress
**PR**: #630
**Type**: Refactor (architecture)
**Stack**: #satellite 8/13. Base: `feature/satellite/client-transports` (#629).

## Initial Discovery

[2026-10-16-livekit-satellite-coder-participant-initial-discovery.md](./2026-10-16-livekit-satellite-coder-participant-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-coder/docs/code-issues/oversized-file-run.md` | ⚠ DURING | Unclaimed. This node replaces four join sites with port calls; it must not grow `run.rs`. Expected to shrink it (token/participant boilerplate leaves). |

## Responsibility

The coder's *serving* participant as a port. Owns `SessionParticipantHost`, `ParticipantJoin`,
`ParticipantOptions`, `ServedParticipant`, `NoParticipantHost` in `tddy-coder`; the new binary crate
`tddy-coder-cli` with `LiveKitParticipantHost`; the move of `merge_participant_metadata_json` to
`tddy-core`; the script updates the crate split requires. Strikes `tddy-coder`; adds `tddy-coder-cli`
to the ratchet's `WIRING`.

## Boundaries

- Does not split `run.rs` beyond replacing the join sites.
- Does not change CLI flags or the binary's name.
- Does not touch the client crates of node 7 or the daemon.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #623 `satellite-1-extension-ports` | The ratchet (`WIRING`, `KNOWN_OFFENDERS`) | Strikes `tddy-coder`, adds `tddy-coder-cli` to `WIRING` | Change the ratchet's logic |
| #629 client-transports | `tddy-room-client` (client join port) | **Not consumed**: the coder *serves* a surface, it does not join as a client. No edge n7 → n8. | Reuse `RoomConnector` |
| #624–#628 | Nothing consumed | — | — |

**As published (wave 2):** `participant_host` (port, `NoParticipantHost`, join/options/served types)
and `spawn_session_participant_with` (todo!()), with failing tests. **Shape correction:** the room
lives in `ParticipantAuth::Credentials { room, .. }`, not on `ParticipantJoin` — a token-authenticated
join's room is the one its token was minted for, and `spawn_session_participant` never had a room to
pass. Not yet published: `run_main(args, host)`, the `run.rs` call-site tests, the
`merge_participant_metadata_json` move, and `tddy-coder-cli` (all green). `WIRING` already names
`tddy-coder-cli`; the ratchet assertions wait on #623's `WorkspaceGraph`.

## Draft PR contract

```rust
// tddy-coder/src/participant_host.rs (lib)
pub enum ParticipantAuth { Credentials { api_key: String, api_secret: String, ttl: Duration }, Token(String) }
pub struct ParticipantJoin { pub url: String, pub room: String, pub identity: String, pub auth: ParticipantAuth }
pub struct ParticipantOptions {
    pub metadata: Option<tokio::sync::watch::Receiver<String>>,
    pub codex_oauth_watch: Option<PathBuf>,
    pub projects_registry_dir: Option<PathBuf>,
    pub reconnect: bool,
    pub shutdown: Arc<AtomicBool>,
}
/// The participant's task; ends when the room does (or on shutdown).
pub struct ServedParticipant { /* JoinHandle<()> */ }
#[async_trait::async_trait]
pub trait SessionParticipantHost: Send + Sync {
    async fn serve(&self, join: ParticipantJoin, surface: tddy_rpc::MultiRpcService,
                   options: ParticipantOptions) -> Result<ServedParticipant, String>;
}
pub struct NoParticipantHost;

// tddy-core::session_participant_metadata
pub fn merge_participant_metadata_json(baseline: &str, update: &str) -> Result<String, serde_json::Error>;

// packages/tddy-coder-cli (new, [[bin]] name = "tddy-coder")
pub struct LiveKitParticipantHost;   // over tddy_livekit::LiveKitParticipant
```

`run_main(args)` gains the host: `run_main(args, host: Arc<dyn SessionParticipantHost>)`. Bodies
`todo!()` until green; the failing tests below are published with the surface.

## Green wave

**Wave:** 2 of 5
**Greenable independently:** yes — library tests use an in-memory host; the binary keeps the
existing LiveKit suites.
**Concurrent with:** nodes 2–7, 9
**Blocks:** none

    n1 → n2..n9   n2, n4 → n10   n2, n3, n4, n7, n10 → n11   n4, n10 → n12   n11, n12 → n13

## Affected Packages

`tddy-coder`, new `tddy-coder-cli`, `tddy-core` (JSON helper), `tddy-livekit` (re-export),
`tddy-e2e` / `tddy-integration-tests` / `tddy-daemon-sandbox` (binary resolution, if needed),
scripts `release`, `test`, `scripts/dev-runtime-binaries.sh`, CI workflow.

## Summary

`tddy-coder` the library is LiveKit-free; `tddy-coder` the binary wires the LiveKit participant.

## Scope

- [ ] Port + null object in `tddy-coder`
- [ ] `merge_participant_metadata_json` → `tddy-core`
- [ ] `run.rs` four join sites + `session_participant` on the port
- [ ] `tddy-coder-cli` crate (binary `tddy-coder`) with `LiveKitParticipantHost`; `tddy-coder` library-only
- [ ] Scripts/CI: `-p tddy-coder` → `-p tddy-coder-cli` where a binary is built
- [ ] Ratchet: strike `tddy-coder`; `WIRING` += `tddy-coder-cli`

## Technical Changes

### State A
- `run.rs:69-72` `LiveKitTokenProvider(Arc<TokenGenerator>)`; joins at `:1980-1987` (token service
  provider), `:2229-2290` (reconnecting participant with `run_with_reconnect_metadata`, or
  token-only `connect` + `spawn_local_participant_metadata_watcher`), `:3550-3660`, `:3680-3687`.
- `session_participant/mod.rs:107-121` `LiveKitParticipant::connect` +
  `spawn_local_participant_metadata_watcher`; `metadata_publisher.rs:16`
  `tddy_livekit::merge_participant_metadata_json`.
- `tddy-livekit/src/participant.rs:48` `merge_participant_metadata_json` (pure JSON);
  `:451` `run_with_reconnect_metadata(url, &TokenGenerator, service, RoomOptions, shutdown,
  metadata_watch, codex_oauth_watch, projects_registry_dir)`.
- `web_server.rs:21` matches only a doc comment.
- Manifest: `[lib]` + `[[bin]]`, `tddy-livekit` `:45` (dev: testkit, livekit `:73-74`).
- Scripts build `-p tddy-coder` (`release:60`, `test:23`); several `tddy-coder/tests/*` locate the
  built binary by path (`cli_args.rs`, `cli_integration.rs`, …).

### State B
Library code calls `host.serve(join, surface, options)`; the binary crate supplies
`LiveKitParticipantHost`, which reproduces both today's modes (reconnecting with credentials; fixed
token with metadata watcher).

### Delta
As Scope.

## Implementation Milestones
- [ ] M1 Port + helper move
- [ ] M2 Call sites switched; in-memory host tests green
- [ ] M3 Binary crate; scripts; LiveKit suites green; ratchet green

## Testing Plan

Library: an in-memory `SessionParticipantHost` records the join and options each code path asks for.
Binary: the existing LiveKit e2e suites (`tddy-e2e` `livekit` feature, coder testkit tests) run the
real host.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_coder_with_api_credentials_serves_its_surface_with_reconnect` | `packages/tddy-coder/tests/participant_host_port.rs` (new) | Join = room/identity/credentials, `reconnect: true`, metadata receiver present |
| `a_coder_given_only_a_token_serves_once_with_that_token` | same | `ParticipantAuth::Token`, `reconnect: false` |
| `a_session_participant_serves_under_its_session_identity` | same | `session_participant` join identity and surface |
| `merging_participant_metadata_overwrites_top_level_keys` | `packages/tddy-core/src/session_participant_metadata.rs` (moved tests or new unit) | Helper behaviour unchanged after the move |
| `the_coder_library_names_no_livekit_crate` | `packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` | `tddy-coder` struck; `tddy-coder-cli` in `WIRING`; both ratchet tests pass |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Serving is not joining.** The coder hosts a surface; node 7's `RoomConnector` is a client join.
  Two ports keep each honest and avoid an n7 → n8 edge (which would step the line back a wave).
- **Binary split (recommended) vs feature-gated optional dependency.** A crate's `[[bin]]`-only
  dependency still appears in its manifest, so the ratchet cannot tell a clean library from a dirty
  crate. Splitting the binary into `tddy-coder-cli` makes the boundary structural; the cost is
  script/CI updates (`-p tddy-coder` → `-p tddy-coder-cli`). The alternative — `tddy-livekit`
  optional behind a default feature with `[[bin]] required-features` — keeps one crate but needs the
  ratchet to understand feature-gated edges. Same question as node 7's open decision; decide once for
  both at wave 2 of node 7.
- **Helper to `tddy-core`.** `merge_participant_metadata_json` is pure JSON over the block type
  `tddy-core` already owns.

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

- [x] Record initial discovery (`2026-10-16-livekit-satellite-coder-participant-initial-discovery.md`)
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
