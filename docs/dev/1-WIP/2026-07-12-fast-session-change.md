# Changeset: Fast Session Change — participant-bound sessions, background terminals, session metadata

**Date**: 2026-07-12
**Branch**: `feat-fast-session-change`
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Packages**: `tddy-web`, `tddy-coder`, `tddy-livekit`, `tddy-daemon`
**Feature PRDs**:
- [Web — Fast Session Change](../ft/web/1-WIP/PRD-2026-07-12-fast-session-change.md)
- [Daemon — Fast Session Change](../ft/daemon/1-WIP/PRD-2026-07-12-fast-session-change.md)
- [Coder — Session Participant RPC & Metadata](../ft/coder/session-participant-rpc.md)

## Affected Packages

**CRITICAL**: List ALL packages with documentation changes:

- **tddy-web**: [README](../../packages/tddy-web/README.md) — per-session runtime registry, background terminals, session-participant RPC routing, participant metadata merge, inspector I/O bytes + last-data-received.
- **tddy-coder**: [README](../../packages/tddy-coder/README.md) — serve session-scoped `ConnectionService` (tools, terminal control) from the LiveKit participant; publish `session` metadata. Delete/signal are daemon-direct (not served by the coder).
- **tddy-livekit**: [`participant-metadata.md`](../../packages/tddy-livekit/docs/participant-metadata.md) — document the `session` metadata key + merge semantics.
- **tddy-daemon**: [`terminal-sessions.md`](../../packages/tddy-daemon/docs/terminal-sessions.md) (feature-facing) — bootstrap/directory boundary + delete/signal stay daemon-direct (no coder relay); `connection_service.rs` gains a daemon-direct caller contract test.

## Related Feature Documentation

- [Session Drawer Screen](../ft/web/session-drawer.md)
- [Web Terminal](../ft/web/web-terminal.md)
- [LiveKit common room: owned project count](../ft/web/livekit-participant-owned-projects.md)
- [Terminal Sessions](../ft/daemon/terminal-sessions.md)
- [Session Participant RPC & Metadata](../ft/coder/session-participant-rpc.md) (new)

## Summary

Bind each attached LiveKit session to its own room participant: a self-contained runtime (Room + Ghostty terminal + session-participant `ConnectionService` client + byte counters + last-received timestamp) kept alive in the background while the user navigates between sessions; the coder participant serves session-scoped RPCs and publishes session metadata; the daemon keeps bootstrap/directory authority and remains the direct target for `DeleteSession` / `SignalSession` (so lifecycle control still works when the coder participant is stuck).

## Background

Switching sessions today unmounts the Ghostty terminal, tears down its LiveKit room, and reconnects/re-resizes on the new selection; switched-away sessions stop streaming. All session-scoped RPCs route through the daemon participant, and the coder's `metadata_tx` is ignored, so cross-host active rows have no metadata without a `ListSessions` fan-out. Fast context switching between parallel sessions is the core operator workflow; this changeset removes the reconnect tax, lets background sessions keep streaming, and makes presence carry the metadata.

## Scope

**High-level deliverables tracking progress throughout development:**

- [ ] **Package Documentation**: Update package READMEs and dev docs (wrap-time — permanent `packages/*/docs/` edits deferred to wrap per workspace rules; `session` metadata key schema captured in this changeset)
- [x] **Implementation**: `SessionRuntimeRegistry` + background terminals (web); coder `ConnectionService` server (tools/control) + metadata publisher + workflow-state tap (`spawn_session_metadata_tap`); livekit `session` key docs (in changeset); daemon delete/signal daemon-direct contract test
- [x] **Testing**: All acceptance + unit tests passing (see Validation Results)
- [x] **Integration**: Cross-package participant RPC + daemon-direct lifecycle + metadata merge verified
- [x] **Technical Debt**: Explicit-disconnect eviction documented as intentional (no cap); coder workflow-state → `session` metadata tap wired (interactive path; headless `--grpc` path FIXME-tracked)
- [x] **Code Quality**: Linting, type checking complete — `cargo fmt` (clean for feature files), `cargo clippy -D warnings` (clean), `cargo test -p tddy-coder` (all pass incl. 8 new tap-mapping tests + LiveKit integration test); code review re-run on metadata-tap changes

## Technical Changes

### State A (Current)

- `useSessionAttachment` holds a single attached session; `SessionsDrawerScreen` renders one `SessionLiveKitTerminal`; switching sessions calls `resetAttachment`, unmounts the terminal, drops the LiveKit Room, reconnects + resizes on the next selection.
- All session-scoped `ConnectionService` RPCs route through the daemon participant (`daemon-{instanceId}`); the coder participant serves only `TerminalService`.
- `tddy-coder` creates a `metadata_tx` watch channel in `run.rs` but never sends on it; its LiveKit participant carries no `session` metadata.
- `useRoomParticipants` exposes raw participant metadata; `SessionManager.mergeActiveAndFetchedSessions` synthesizes cross-host active rows from identity with a short-id label and no goal/state/agent/model.
- The inspector shows session metadata from `ListSessions` enrichment only; no per-session I/O byte counters and no "last data received" timestamp.

### State B (Target)

- A `SessionRuntimeRegistry` keyed by `sessionId` holds one `SessionRuntimeState` per attached session: LiveKit `Room`, `ConnectionService` client bound to `daemon-{instanceId}-{sessionId}`, `GhosttyTerminalHandle`, traffic meter, byte counters (in/out), `lastDataReceivedAt`, terminal control state. Background attachments persist until explicit disconnect (no cap).
- `SessionsDrawerScreen` mounts one `<SessionRuntime>` per attached session; the focused session's terminal is CSS-visible, others are `display:none` but still subscribed to `streamTerminalIO`. Focus switch = no unmount, no reconnect, no resize.
- Session-scoped RPCs (`ListExecTools`, `ListSessionToolCalls`, `ExecuteTool`, `ClaimTerminalControl`, `WatchTerminalControl`, VNC, screen-sharing) route through the session participant; bootstrap/directory **and lifecycle** RPCs (`StartSession`, `ConnectSession`, `ResumeSession`, `ListSessions`, `ListProjects`, `ListAgents`, `ListTools`, `ListEligibleDaemons`, `ListProjectBranches`, `DeleteSession`, `SignalSession`) stay on the daemon participant. `DeleteSession` / `SignalSession` are daemon-direct (not relayed through the coder) so they still work when the coder participant is stuck — the daemon owns process teardown.
- `tddy-coder` serves `ConnectionService` session-scoped methods (tools, terminal control) from its participant and publishes a `session` metadata block on state transitions (shallow-merged with `owned_project_count` / `codex_oauth`). It does **not** serve or relay `DeleteSession` / `SignalSession`.
- `useRoomParticipants` parses a `session` metadata field; `SessionManager` overlays it onto synthesized and fetched rows (presence-driven, no `ListSessions` fan-out for active rows).
- The inspector shows bytes in/out (per-session traffic meter) and a "last data received: Ns ago" relative timestamp that advances while open; `lastDataReceivedAt` is wired from the LiveKit transport's `DataReceived` events and updates in the background.
- **Dual source**: for an attached LiveKit session, bytes in/out + `lastDataReceivedAt` come from the per-session runtime (traffic meter + `DataReceived`). For a session with **no LiveKit participant** (stopped tddy-coder session, or claude-cli / cursor-cli / workspace sessions that never join a room), the inspector falls back to `SessionEntry` fields (`bytes_in`, `bytes_out`, `last_data_received_at`) populated by the daemon `ListSessions` RPC — the daemon reports live counters for `GrpcSessionTerminal` sessions it owns, and zero / empty for stopped tddy-coder sessions. The inspector renders the live runtime values when a runtime exists, else the daemon-sourced `SessionEntry` values.

### Delta (What's Changing)

#### tddy-web

- **New**: `src/components/sessions/sessionRuntimeRegistry.ts` — observable store keyed by `sessionId` (`SessionRuntimeState`: Room, session-participant `ConnectionService` client, terminal handle, traffic meter, byte counters, `lastDataReceivedAt`, control state); add/focus/disconnect; explicit-disconnect eviction.
- **New**: `src/components/sessions/sessionParticipantRpcClient.ts` — build a `ConnectionService` client via `liveKitFactory(room, sessionServerIdentity)` for the session participant.
- **New**: `src/lib/sessionParticipantMetadata.ts` — parse the `session` JSON block from participant metadata; tolerate missing keys / older empty metadata.
- **New**: `src/components/sessions/lastDataReceivedFormat.ts` — relative-time formatter ("5s ago", "2m ago") for `lastDataReceivedAt`.
- **Modified**: `SessionsDrawerScreen.tsx` — mount one runtime per attached session; focus switch = CSS visibility, no unmount; route session-scoped RPCs through the session participant; keep bootstrap/directory on the daemon.
- **Modified**: `SessionMainPane.tsx` — render the focused runtime's terminal; keep background runtimes mounted in a hidden layer.
- **Modified**: `SessionInspectorDrawer.tsx` — Details tab gains bytes in/out + "last data received: Ns ago"; renders from the per-session runtime when a live runtime exists, else falls back to `SessionEntry.bytes_in` / `bytes_out` / `last_data_received_at` (daemon `ListSessions`-sourced).
- **Modified**: `useRoomParticipants.ts` — `RoomParticipant` gains a parsed `session` metadata field.
- **Modified**: `sessionManager.ts` — `mergeActiveAndFetchedSessions` overlays parsed `session` metadata onto synthesized and fetched rows.

#### tddy-coder

- **New**: `src/session_participant/connection_service_participant.rs` — session-scoped `ConnectionService` handlers (`ListExecTools`, `ListSessionToolCalls`, `ExecuteTool`, `ClaimTerminalControl`, `WatchTerminalControl`). `DeleteSession` / `SignalSession` are intentionally NOT served here (daemon-direct).
- **New**: `src/session_participant/metadata_publisher.rs` — `Changeset` + `SessionMetadata` → `session` JSON → push to `metadata_tx`; merge preserves `owned_project_count` / `codex_oauth`.
- **New**: `src/session_participant/mod.rs` — `spawn_session_participant` + `SessionParticipantOptions`.
- **Modified**: `src/run.rs` — register `ConnectionService` on the participant `ServiceEntry` list (both `livekit_multi` and single-token paths); wire the previously-ignored `metadata_tx`.

#### tddy-livekit

- **Modified**: `docs/participant-metadata.md` — document the `session` metadata key + shallow-merge semantics (sibling of `owned_project_count` / `codex_oauth`). No new public API; the merge helper already exists.
  - **`session` key schema** (wrap-time content for `participant-metadata.md`): a JSON object published by `tddy-coder`'s participant on workflow transitions, shallow-merged with the existing `owned_project_count` / `codex_oauth` fragments via `merge_participant_metadata_json`. Fields:
    - `session_id: string` — the session the block describes (matches the identity suffix `daemon-{instanceId}-{sessionId}`).
    - `workflow_goal: string` — the PRD goal line (empty until the first plan transition).
    - `workflow_state: string` — the current `WorkflowState` label (e.g. `idle`, `planning`, `coding`, `verifying`).
    - `agent: string` — the active agent / backend name (e.g. `claude`, `codex`).
    - `model: string` — the active model identifier.
  - Tolerated as absent on older participants; `useRoomParticipants` → `parseSessionParticipantMetadata` treats missing/empty/invalid JSON as no overlay.

#### tddy-daemon

- **Modified**: `src/connection_service.rs` — no behaviour change; `DeleteSession` / `SignalSession` remain daemon-direct (the web calls them on `daemon-{instanceId}`). Add a contract test asserting a caller with a valid `session_token` clears auth and reaches session processing (regression guard for the daemon-direct path).
- **Modified**: `src/connection_service.rs` (or session-enrichment path) — populate new `SessionEntry` fields `bytes_in` / `bytes_out` / `last_data_received_at` from the `GrpcSessionTerminal` traffic meter for claude-cli / cursor-cli / workspace sessions the daemon owns; zero / empty for tddy-coder sessions with no LiveKit participant.
- **Proto**: `packages/tddy-service/proto/connection.proto` `SessionEntry` — add `uint64 bytes_in = 24;`, `uint64 bytes_out = 25;`, `string last_data_received_at = 26;` (epoch-millis string, empty when never received).
- **Docs**: `docs/ft/daemon/terminal-sessions.md` (via PRD wrap) — bootstrap/directory boundary; delete/signal stay daemon-direct (no coder relay).

## Implementation Milestones

- [x] Milestone 1: `SessionRuntimeRegistry` + per-session Room/terminal mounting with background streaming (web)
- [x] Milestone 2: Session-participant RPC routing for session-scoped methods (web)
- [x] Milestone 3: Coder serves `ConnectionService` from its participant (coder)
- [x] Milestone 4: Coder publishes `session` metadata on transitions (coder) — publisher + workflow-state tap wired on the interactive path (`spawn_session_metadata_tap` subscribes to `PresenterEvent`s and publishes `SessionMetadata` on `metadata_tx`); headless `--grpc` path FIXME-tracked
- [x] Milestone 5: Delete/signal stay daemon-direct (web routes to `daemon-{instanceId}`; no coder relay) + daemon contract test
- [x] Milestone 6: Sessions list overlays participant `session` metadata (web + livekit docs)
- [x] Milestone 7: Inspector I/O bytes + last-data-received (web) — live runtime for attached sessions; daemon-RPC (`SessionEntry`) fallback for sessions with no LiveKit participant.
- [x] Milestone 8: livekit `session` metadata key docs (livekit) — schema captured in this changeset; permanent `participant-metadata.md` edit at wrap time.

## Testing Plan

### Testing Strategy

**Primary Test Approach**: Cypress component acceptance tests (web) + Rust integration tests against `LIVEKIT_TESTKIT_WS_URL` (coder/daemon) + Rust/TS unit tests for pure logic. Web component tests use `mountWithRpc` + an in-memory RPC backend (no `cy.intercept`).

### Option 1: Cypress component acceptance (web)

**Test Level**: Component (in-memory RPC backend)
**Scope**: session runtime registry behaviour, participant RPC routing, inspector byte/last-received, sessions list metadata, start/resume still daemon-routed.

**Assertions**:
- [ ] Background terminal stays mounted and receives bytes after a focus switch (registry + runtime).
- [ ] Session-scoped RPCs target the session participant identity (in-memory backend records target identity).
- [ ] Inspector shows bytes in > 0, bytes out, and an advancing "last data received" relative string (attached session: live runtime).
- [ ] Inspector renders bytes in/out + last-data-received from the daemon `ListSessions` `SessionEntry` fields when no LiveKit participant is present (inactive fallback).
- [ ] A participant with `session` metadata yields a drawer row with goal/state/agent/model and no `ListSessions` call for that row.
- [ ] Start/Resume target `daemon-{instanceId}` (regression guard).

### Option 2: Rust integration acceptance (coder/daemon)

**Test Level**: Integration (LiveKit testkit)
**Scope**: coder participant answers session-scoped `ConnectionService` (tools, terminal control); metadata published after a transition. Delete/signal are daemon-direct (covered by web Cypress + daemon unit test).

**Assertions**:
- [ ] `coder_serves_connection_service_from_participant`: spawned coder answers `ListExecTools` / `ExecuteTool` / `ClaimTerminalControl` over its identity. (Delete/signal are daemon-direct and covered separately.)
- [ ] `coder_publishes_session_metadata_to_participant`: after a workflow state transition the participant's metadata JSON contains the `session` block with goal/state/agent/model.

### Option 3: Unit (per package)

**Scope**: registry, metadata merge/parser, last-received format, participant RPC client (web); connection_service_participant / metadata_publisher (coder); livekit merge preserves `session` key; daemon delete/signal daemon-direct contract.

**Assertions**: deterministic input/output for pure logic; in-process harnesses for handlers.

### Coverage Requirements

- [ ] **Happy path**: focus switch keeps both terminals alive; delete/signal reach the daemon directly; metadata publishes.
- [ ] **Error scenarios**: delete/signal daemon errors surface to the web; metadata parser tolerates missing/empty.
- [ ] **Edge cases**: empty metadata; older participants without `session`; explicit disconnect evicts only that runtime; inspector falls back to daemon `SessionEntry` bytes when no LiveKit participant.
- [ ] **Integration points**: web → coder participant (tools/control); web → daemon participant (delete/signal/bootstrap); livekit merge coexistence.
- [ ] **Actual effects verification**: background bytes increase after switch; delete/signal hit the daemon participant; metadata visible on participant.

## Acceptance Tests

### tddy-web (Cypress component)

- [x] `cypress/component/SessionRuntimeRegistryBackgroundTerminals.cy.tsx`
- [x] `cypress/component/SessionParticipantRpcRouting.cy.tsx`
- [x] `cypress/component/SessionInspectorByteCountAndLastReceived.cy.tsx`
- [x] `cypress/component/SessionInspectorInactiveDaemonSourced.cy.tsx` (req 5 dual source — daemon `SessionEntry` fallback when no LiveKit participant)
- [x] `cypress/component/SessionsListParticipantMetadata.cy.tsx`
- [x] `cypress/component/SessionStartResumeStillDaemonRouted.cy.tsx`

### tddy-coder / tddy-daemon (Rust integration)

- [x] `coder_serves_connection_service_from_participant`
- [x] `coder_publishes_session_metadata_to_participant`

## Unit Tests

- [x] `tddy-web/src/components/sessions/sessionRuntimeRegistry.test.ts`
- [x] `tddy-web/src/components/sessions/sessionRuntimeRegistryMetadataMerge.test.ts`
- [x] `tddy-web/src/lib/sessionParticipantMetadata.test.ts`
- [x] `tddy-web/src/components/sessions/lastDataReceivedFormat.test.ts`
- [x] `tddy-web/src/components/sessions/sessionParticipantRpcClient.test.ts`
- [x] `tddy-coder src/.../connection_service_participant.rs` (module tests)
- [x] `tddy-coder src/.../metadata_publisher.rs` (module tests)
- [x] `tddy-coder src/session_participant/connection_service_participant.rs` (module tests)
- [x] `tddy-coder src/session_participant/metadata_publisher.rs` (module tests)
- [x] `tddy-livekit tests/participant_metadata_unit.rs` (extend — `session` key preserved across merges)
- [x] `tddy-daemon src/connection_service.rs` (extend — daemon-direct delete/signal caller contract)

## Validation Results

### Green phase — all feature tests passing

**Rust** (`cargo test -p tddy-coder -p tddy-daemon -p tddy-livekit -p tddy-service -p tddy-core` against `LIVEKIT_TESTKIT_WS_URL=ws://127.0.0.1:32768`):
- `tddy-core`: 253 lib + all integration suites pass.
- `tddy-coder`: 62 lib + all integration suites pass, incl. `coder_serves_connection_service_from_participant` (1), `coder_publishes_session_metadata_to_participant` (1), and 8 new `metadata_publisher::tests` tap-mapping tests (`apply_session_metadata_event` for `BackendSelected` / `GoalStarted` / `StateChanged` / `ModeChanged` elicitation + running fallback / `WorkflowComplete` ok+err / irrelevant-event no-op, plus `a_default_session_metadata` seed).
- `tddy-daemon`: 288 lib pass, incl. `delete_session_unit_accepts_daemon_direct_caller_with_valid_token`. 1 pre-existing failure (`sandbox_session::tests::dial_and_bridge_drives_run_host_relay_over_a_stdio_sandbox_client` panics with "build tddy-sandbox-runner first" — missing pre-built binary; unrelated to this feature).
- `tddy-livekit`: 11 lib + `participant_metadata_acceptance` (3) + `participant_metadata_unit` (1) + rpc suites pass.
- `tddy-service`: 39 lib pass.

**Web — Cypress component** (`bun run cypress:component`): 453 of 460 pass across 81 specs. All 6 new feature specs pass; all affected existing specs pass (`SessionsDrawerAcceptance`, `SessionInspectorAcceptance`, `TerminalControlAcceptance`, `SessionMainPaneLiveKitTerminal`, etc.). The single failing spec `PlannedPrRowInternalStatusAcceptance` (3 tests) is **pre-existing** — confirmed failing on the merge-base `2bd16ba7` (unrelated to this feature; PR-stack internal-status badge).

**Web — bun unit** (`bun test packages/tddy-web/src/components/sessions`): 96 pass, 0 fail.

### Lint / format

- `cargo fmt --check -p tddy-coder -p tddy-daemon -p tddy-livekit -p tddy-service -p tddy-core`: clean (applied `cargo fmt`).
- `cargo clippy -p tddy-coder -p tddy-daemon -p tddy-livekit -p tddy-service -p tddy-core --all-targets -- -D warnings`: clean. Fixed one pre-existing `clippy::assertions_on_constants` violation in `tddy-daemon/src/connection_service.rs` (frame-limit test) by wrapping the constant asserts in a `const _: () = { ... }` block (behavior-preserving compile-time check).

### Pre-existing, unrelated failures (not introduced by this changeset)

- ~~`tddy-sandbox-darwin` test compile error (`missing field 'resume' in `SandboxRunnerArgs`)~~ — **fixed by this changeset**: added `resume: false` to the three `SandboxRunnerArgs` struct-literal construction sites that were stale after the `resume` field landed on `tddy-sandbox-runner` (`packages/tddy-sandbox-darwin/tests/sandbox_runner_acceptance.rs`, `…/sandbox_runner_behavior_acceptance.rs`, `packages/tddy-integration-tests/tests/sandbox_egress_relay_tls.rs`). All `tddy-sandbox-darwin` tests now compile and pass on Linux (7 pass; macOS-only suites cfg-gated to 0 on Linux); `cargo clippy -p tddy-sandbox-darwin --tests -- -D warnings` clean. One behavior test additionally requires the pre-built `tddy-demo-tui` binary (`cargo build -p tddy-demo-tui`), analogous to the `tddy-sandbox-runner` binary requirement below.
- `tddy-daemon sandbox_session` test ("build tddy-sandbox-runner first") — missing pre-built binary; present on merge-base.
- `PlannedPrRowInternalStatusAcceptance.cy.tsx` — present on merge-base.

### Production readiness

- Deferred work is FIXME-marked with the `2026-07-12-fast-session-change` tag:
  - `packages/tddy-coder/src/run.rs` (headless `--grpc` path) — wire real exec tool catalog + executor + the `session` metadata tap into that path's own thread/runtime (the interactive path now spawns `spawn_session_metadata_tap`).
  - `packages/tddy-coder/src/session_participant/connection_service_participant.rs` — `ExecuteTool` returns an honest "not yet wired" error; `coder_session_tool_catalog()` returns an empty catalog until the tool-engine integration lands.
- No `println!`/`eprintln!` in TUI code paths; no test-only `cfg(test)` branches in production code.
- The `liveKitFactoryIsOverridden` seam in `SessionsDrawerScreen.buildSessionClient` is a deliberate DI adaptation: in production the session's own LiveKit `Room` (captured via `onRoom`) is the transport room; the common-room fallback only applies when the transport factory is overridden (test doubles that ignore their `room` argument). Production behaviour is unchanged (the fallback path is unreachable with the real factory).

## Technical Debt & Production Readiness

- [ ] Explicit-disconnect eviction is intentional (no cap) — document memory-growth trade-off in the web feature doc at wrap time.
- [ ] claude-cli / cursor-cli / workspace sessions remain on the gRPC path; background terminals for them are out of scope (see Future Enhancements below).

## Decisions & Trade-offs

- **RPC routing (confirmed)**: tddy-coder serves session-scoped `ConnectionService` (tools, terminal control, VNC, screen-sharing) from its participant; `DeleteSession` / `SignalSession` are **daemon-direct** (the web calls `daemon-{instanceId}`), not relayed through the coder, so lifecycle control still works when the coder participant is stuck; bootstrap + directory RPCs stay on the daemon. Maximises session-screen isolation; keeps the daemon the single authority for process teardown.
- **Eviction (confirmed)**: background attachments persist until explicit disconnect (no cap). Simplest model; memory cost accepted; heartbeat-based auto-release deferred to Future Enhancements.
- **Scope**: LiveKit-backed (tddy-coder) sessions only for req 1–3 and 5; non-LiveKit sessions keep their current path and still benefit from `ListSessions` disk enrichment.

## Refactoring Needed

### From @red (TDD Red Phase)

- [x] Runtime layer terminal mounting (one mounted terminal per attached session, focused visible) — done in `SessionMainPane.tsx`.
- [x] Lazy session-scoped `ConnectionService` client (`buildSessionClient`) — done in `SessionsDrawerScreen.tsx`.
- [x] Inspector `bytes_in` / `bytes_out` / `last_data_received_at` — dual source (live runtime + daemon `SessionEntry`) done.
- [x] Daemon-direct `DeleteSession` / `SignalSession` routing — done (web keeps lifecycle on `daemon-{instanceId}`; daemon contract test added).
- [x] Coder `ConnectionService` participant server — done (`spawn_session_participant` + `SessionConnectionServiceRpc`).
- [x] Coder `session` metadata publisher — done (`metadata_publisher.rs`); workflow-state tap wired on the interactive path (`spawn_session_metadata_tap`). Headless `--grpc` path FIXME-tracked.

### Follow-up (tracked, not blocking PR)

- [x] `g-coder-metadata-tap`: wire the coder's workflow-state transitions to the `session` metadata block on the participant — done for the interactive path (`spawn_session_metadata_tap` subscribes to `PresenterEvent`s via `usage_event_tx.subscribe()`, maps them via `apply_session_metadata_event`, and publishes `SessionMetadata` on `metadata_tx`; seeded with agent/model/recipe/repo_path from CLI args). Headless `--grpc` path remains FIXME-tracked (separate thread/runtime).
- [ ] `CoderSessionToolExecutor.execute`: bridge `ExecuteTool(tool_name, args_json)` to the coder's tool engine (FIXME in `connection_service_participant.rs`). Currently returns an honest "not yet wired" error; `coder_session_tool_catalog()` returns an empty catalog.

## References

- Web PRD: [docs/ft/web/1-WIP/PRD-2026-07-12-fast-session-change.md](../ft/web/1-WIP/PRD-2026-07-12-fast-session-change.md)
- Daemon PRD: [docs/ft/daemon/1-WIP/PRD-2026-07-12-fast-session-change.md](../ft/daemon/1-WIP/PRD-2026-07-12-fast-session-change.md)
- Coder feature doc: [docs/ft/coder/session-participant-rpc.md](../ft/coder/session-participant-rpc.md)
- Related changeset: [byte-traffic.md](./byte-traffic.md) (per-session traffic meter this work builds on)

## Future Enhancements (out of scope)

- Per-terminal zoom scoping across background terminals (already noted in `web-terminal.md` Future Scope).
- Heartbeat-based auto-release of background attachments when a browser tab closes (explicit-only eviction chosen for now).
- claude-cli / cursor-cli / workspace background terminals (no LiveKit participant; would need a gRPC-equivalent background stream).
