# Changeset: Multi-Host Daemon Selection in tddy-web

**Date**: 2026-03-28
**Status**: 🚧 In Progress
**Type**: Feature

## Affected Packages

- **tddy-service**: [proto/connection.proto](../../../packages/tddy-service/proto/connection.proto) — New `ListEligibleDaemons` RPC; `daemon_instance_id` fields on `StartSessionRequest` and `SessionEntry`
- **tddy-daemon**: [src/multi_host.rs](../../../packages/tddy-daemon/src/multi_host.rs), [src/connection_service.rs](../../../packages/tddy-daemon/src/connection_service.rs) — LiveKit-based daemon discovery replacing stub; `ListEligibleDaemons` handler; `StartSession` host routing via LiveKit RPC
- **tddy-web**: [src/components/ConnectionScreen.tsx](../../../packages/tddy-web/src/components/ConnectionScreen.tsx) — Host dropdown per project; host column in session tables; `ListEligibleDaemons` RPC call

## Related Feature Documentation

- [PRD: Multi-Host Daemon Selection](../../ft/web/1-WIP/PRD-2026-03-28-multi-host-daemon-selection.md)
- [Web Terminal](../../ft/web/web-terminal.md) — ConnectionScreen UX
- [Daemon Project Concept](../../ft/daemon/project-concept.md) — Project-centric connection model
- [PRD: tddy-daemon](../../ft/daemon/1-WIP/PRD-2026-03-19-tddy-daemon.md) — Multi-user daemon binary

## Summary

Add multi-host daemon selection to `tddy-web` and `tddy-daemon`. Daemons discover each other via the shared LiveKit common room. The gateway daemon (serving the web bundle) exposes a `ListEligibleDaemons` RPC. The `ConnectionScreen` gains a per-project "Host" dropdown. `StartSession` routes to the selected peer daemon via LiveKit RPC. Session tables display which host each session runs on.

## Background

The `feature/multi-host-daemon-selection` branch already has daemon-side foundation:

- `daemon_instance_id` in config → LiveKit identity `daemon-{instance_id}-{session_id}`
- `multi_host.rs` with `EligibleDaemonSource` trait and `StubEligibleDaemonSource` (returns only local)
- `ProjectData.host_repo_paths` for per-host repo paths
- Multi-host-safe session deletion

Missing pieces: no peer discovery (stub only), no daemon-to-daemon RPC for spawn delegation, no `ListEligibleDaemons` RPC, and no web UI for host selection.

## Scope

**High-level deliverables tracking progress throughout development:**

- [ ] **Proto changes**: `ListEligibleDaemons` RPC, `daemon_instance_id` on `StartSessionRequest` and `SessionEntry`
- [ ] **Daemon discovery**: LiveKit-based `EligibleDaemonSource` replacing stub (daemons join common room with control identity)
- [ ] **Daemon-to-daemon RPC**: Spawn delegation via LiveKit data channel to peer daemons
- [ ] **ListEligibleDaemons handler**: Wired into `ConnectionService`
- [ ] **StartSession host routing**: Route to selected peer or spawn locally
- [ ] **Web UI — Host dropdown**: Per-project dropdown in `ProjectSessionOptions`
- [ ] **Web UI — Session host column**: Show `daemon_instance_id` label in session tables
- [ ] **Web codegen**: Regenerate `connection_pb.ts` after proto changes
- [ ] **Testing**: All acceptance tests passing
- [ ] **Code Quality**: Linting, type checking, and code review complete

## Technical Changes

### State A (Current)

**Proto** (`connection.proto`):
- `ConnectionService` has `ListTools`, `ListSessions`, `ListProjects`, `CreateProject`, `StartSession`, `ConnectSession`, `ResumeSession`, `SignalSession`, `DeleteSession`
- `StartSessionRequest` has `session_token`, `tool_path`, `project_id`, `agent`
- `SessionEntry` has `session_id`, `created_at`, `status`, `repo_path`, `pid`, `is_active`, `project_id`

**Daemon** (`multi_host.rs`):
- `EligibleDaemonSource` trait with `list_eligible_daemons()` returning `Vec<EligibleDaemonInfo>`
- `StubEligibleDaemonSource` returns a single entry: local hostname-based ID
- No daemon presence in LiveKit common room as a control participant
- No daemon-to-daemon RPC

**Web** (`ConnectionScreen.tsx`):
- `ProjectSessionOptions` has Tool dropdown, Backend dropdown, Debug checkbox
- `ProjectSessionForm` = `{ toolPath, agent, debugLogging }`
- Session tables have columns: ID, Date, Status, Repo, PID, Actions
- All RPC calls go to same-origin `/rpc` (gateway daemon)
- No host selection capability

### State B (Target)

**Proto** (`connection.proto`):
- New `ListEligibleDaemons` RPC on `ConnectionService`
- `StartSessionRequest` gains `daemon_instance_id` field (field 5)
- `SessionEntry` gains `daemon_instance_id` field (field 8)
- New messages: `ListEligibleDaemonsRequest`, `ListEligibleDaemonsResponse`, `EligibleDaemonEntry`

**Daemon** (`multi_host.rs` + new modules):
- Daemon joins LiveKit common room with identity `daemon-ctrl-{instance_id}` (distinct from session participant identities `daemon-{instance_id}-{session_id}`)
- `LiveKitEligibleDaemonSource` lists peers by scanning common room participants matching `daemon-ctrl-*` pattern, reading metadata JSON for labels
- Daemon-to-daemon RPC via LiveKit data channel: gateway sends spawn request to peer's `daemon-ctrl-{id}` identity; peer spawns process and returns session metadata
- `ConnectionService.list_eligible_daemons` wired to the LiveKit-based source
- `ConnectionService.start_session` checks `daemon_instance_id`: if local → spawn locally; if peer → delegate via LiveKit RPC

**Web** (`ConnectionScreen.tsx`):
- `ProjectSessionForm` gains `daemonInstanceId` field
- `ProjectSessionOptions` gains "Host" dropdown populated from `ListEligibleDaemons`
- `handleStartSession` passes `daemonInstanceId` in `StartSession` request
- Session tables gain "Host" column showing the daemon label
- When only one daemon is available, host dropdown shows single option (no special hiding)
- `connection_pb.ts` regenerated with new types

### Delta (What's Changing)

#### tddy-service (proto)
- **API**: Add `ListEligibleDaemons` RPC to `ConnectionService`
- **Messages**: Add `EligibleDaemonEntry` (`instance_id`, `label`, `is_local`), request/response wrappers
- **Fields**: `StartSessionRequest.daemon_instance_id` (field 5), `SessionEntry.daemon_instance_id` (field 8)

#### tddy-daemon
- **Discovery**: Replace `StubEligibleDaemonSource` with `LiveKitEligibleDaemonSource` that reads common room participants
- **Presence**: Daemon joins common room with `daemon-ctrl-{instance_id}` identity and metadata JSON (`{ "label": "hostname (this daemon)", "version": "..." }`)
- **Peer RPC**: New internal service for daemon-to-daemon spawn requests over LiveKit data channel (reuses `tddy-livekit` `RpcClient`/`RpcBridge` infrastructure)
- **Connection service**: Wire `ListEligibleDaemons` handler; route `StartSession` to peer when `daemon_instance_id` targets a remote daemon
- **Session metadata**: Populate `daemon_instance_id` on `SessionEntry` from `.session.yaml` or from the daemon that owns the session

#### tddy-web
- **Components**: `ProjectSessionOptions` gains Host dropdown; `ProjectSessionForm` gains `daemonInstanceId`
- **Data loading**: `ConnectionScreen` calls `ListEligibleDaemons` on load and stores result alongside tools/sessions/projects
- **Start session**: `handleStartSession` passes `daemonInstanceId` to `StartSession`
- **Session display**: Session tables gain "Host" column (`s.daemonInstanceId` mapped to daemon label)
- **Codegen**: Regenerate `connection_pb.ts` from updated proto

## Implementation Milestones

### Milestone 1: Proto + Codegen
- [ ] Add `ListEligibleDaemons` RPC and messages to `connection.proto`
- [ ] Add `daemon_instance_id` to `StartSessionRequest` (field 5)
- [ ] Add `daemon_instance_id` to `SessionEntry` (field 8)
- [ ] Run Rust codegen (`cargo build -p tddy-service`)
- [ ] Run TypeScript codegen for `tddy-web` (`connection_pb.ts`)

### Milestone 2: Daemon Discovery (LiveKit-based)
- [ ] Daemon joins common room with `daemon-ctrl-{instance_id}` identity on startup
- [ ] Implement `LiveKitEligibleDaemonSource` — scan room participants for `daemon-ctrl-*` identities
- [ ] Parse participant metadata for labels
- [ ] Wire `ListEligibleDaemons` handler into `ConnectionService`

### Milestone 3: Daemon-to-Daemon RPC
- [ ] Define internal spawn request/response proto (or use existing `RpcBridge` pattern)
- [ ] Gateway daemon sends spawn request to peer via LiveKit data channel targeting `daemon-ctrl-{peer_id}`
- [ ] Peer daemon handles spawn request: spawns process, returns session metadata
- [ ] Gateway relays response to web client

### Milestone 4: StartSession Host Routing
- [ ] `ConnectionService.start_session` reads `daemon_instance_id` from request
- [ ] If empty or local → spawn locally (existing behavior)
- [ ] If peer → delegate to peer via daemon-to-daemon RPC
- [ ] Populate `daemon_instance_id` on `SessionEntry` when listing sessions

### Milestone 5: Web UI — Host Dropdown
- [ ] Call `ListEligibleDaemons` on authenticated load, store in state
- [ ] Add `daemonInstanceId` to `ProjectSessionForm`
- [ ] Add "Host" dropdown to `ProjectSessionOptions`
- [ ] Pass `daemonInstanceId` in `handleStartSession`

### Milestone 6: Web UI — Session Host Column
- [ ] Add "Host" column to project session tables
- [ ] Add "Host" column to orphan session table
- [ ] Map `daemonInstanceId` to daemon label for display

## Testing Plan

### Testing Strategy

**Primary Test Approach:** Integration tests (daemon) + Cypress component tests (web)

This changeset spans three packages. The daemon-side changes (discovery, routing) are best tested with integration tests against a real or mock LiveKit room. The web UI changes (dropdown, column) are best tested with Cypress component tests against mocked RPC responses.

### Option 1: Daemon Integration Tests

**Test Level**: Integration
**Why**: Daemon discovery and routing involve LiveKit room interactions and RPC dispatch — need real service interactions.

**Scope**:
- `ListEligibleDaemons` returns local daemon when no common room
- `ListEligibleDaemons` returns multiple daemons when peers are present
- `StartSession` with empty `daemon_instance_id` spawns locally
- `StartSession` with peer `daemon_instance_id` routes to peer
- `SessionEntry` includes `daemon_instance_id`

**Assertions**:
- [ ] `ListEligibleDaemons` response contains at least one entry with `is_local: true`
- [ ] Entry `instance_id` matches configured `daemon_instance_id` (or hostname)
- [ ] `StartSession` with local ID produces session with matching `daemon_instance_id`
- [ ] `SessionEntry.daemon_instance_id` is non-empty for all sessions

### Option 2: Daemon Unit Tests

**Test Level**: Unit
**Why**: Discovery source, identity formatting, metadata parsing are pure logic.

**Scope**:
- `LiveKitEligibleDaemonSource` participant filtering
- Control identity formatting (`daemon-ctrl-{id}`)
- Metadata JSON parsing for labels
- `StartSessionRequest` routing decision (local vs peer)

**Assertions**:
- [ ] Participants not matching `daemon-ctrl-*` are excluded
- [ ] Local daemon is marked `is_local: true`
- [ ] Invalid metadata JSON results in default label
- [ ] Empty `daemon_instance_id` routes to local

### Option 3: Cypress Component Tests (Web)

**Test Level**: Component (Cypress)
**Why**: UI dropdown behavior and table column rendering with mocked RPC.

**Scope**:
- Host dropdown appears in `ProjectSessionOptions`
- Dropdown populated from `ListEligibleDaemons` response
- Selected host passed to `StartSession`
- Session table shows "Host" column
- Single-daemon scenario: dropdown shows one option

**Assertions**:
- [ ] `[data-testid="host-select-{projectId}"]` exists and has options matching eligible daemons
- [ ] Starting session sends `daemonInstanceId` in RPC request
- [ ] Session table rows display host label in "Host" column
- [ ] Default selection is the local daemon (first entry / `is_local`)

## Acceptance Tests

### tddy-service
- [ ] **Unit**: Proto compiles with new fields and RPC (`cargo build -p tddy-service`)

### tddy-daemon
- [ ] **Integration**: `ListEligibleDaemons` returns local daemon entry (multi_host_acceptance.rs)
- [ ] **Integration**: `StartSession` with empty `daemon_instance_id` spawns locally — no regression (acceptance_daemon.rs or similar)
- [ ] **Unit**: `LiveKitEligibleDaemonSource` filters participants correctly (multi_host.rs tests)
- [ ] **Unit**: Control identity `daemon-ctrl-{id}` formatting (multi_host.rs tests)
- [ ] **Unit**: Routing decision: local vs peer based on `daemon_instance_id` (connection_service.rs tests)
- [ ] **Integration**: `SessionEntry` includes `daemon_instance_id` from session metadata

### tddy-web
- [ ] **Cypress component**: Host dropdown renders in `ProjectSessionOptions`
- [ ] **Cypress component**: Host dropdown options match `ListEligibleDaemons` response
- [ ] **Cypress component**: `StartSession` request includes selected `daemonInstanceId`
- [ ] **Cypress component**: Session table has "Host" column with daemon labels
- [ ] **Cypress component**: Single daemon → dropdown shows one option, defaults to it

## Technical Debt & Production Readiness

_(To be tracked during implementation)_

## Decisions & Trade-offs

### D1: LiveKit common room for daemon discovery (over static config)

**Chosen**: Daemons discover peers via shared LiveKit common room presence.

**Why**: The common room already exists in the architecture. Daemons and spawned sessions already use it. Discovery is live — peers joining/leaving are reflected automatically. No need for an additional configuration layer listing peer URLs.

**Trade-off**: Depends on LiveKit being available and configured with `common_room`. Single-daemon setups without LiveKit still work (stub returns local only).

### D2: Gateway routing via LiveKit RPC (over direct browser-to-peer connection)

**Chosen**: The web app always talks to the gateway daemon. The gateway delegates `StartSession` to the selected peer via LiveKit data channel RPC.

**Why**: Avoids CORS complexity, multiple daemon endpoints, and certificate management. The web bundle is served by one daemon; all RPC goes to that origin. LiveKit handles the transport between daemons — the same `RpcClient`/`RpcBridge` infrastructure used for terminal streaming.

**Trade-off**: Gateway is a routing bottleneck for the initial spawn RPC (but not for ongoing terminal streaming, which goes directly via LiveKit room). If the gateway goes down, peers are unreachable from the web UI.

### D3: `daemon-ctrl-{instance_id}` identity for control plane (separate from session identities)

**Chosen**: Daemons join the common room with `daemon-ctrl-{instance_id}` for discovery and peer RPC, separate from session participant identities `daemon-{instance_id}-{session_id}`.

**Why**: Clean separation of control-plane (daemon management) from data-plane (terminal streaming). Discovery can filter on `daemon-ctrl-*` without confusing session participants as daemons.

### D4: Per-project host dropdown (not global selector)

**Chosen**: Host is selected per-project alongside Tool and Backend in `ProjectSessionOptions`.

**Why**: Different projects may live on different hosts. A per-project dropdown lets users choose the host that has the project's repo. Consistent with how Tool and Backend already work — per-session, not stored on the project.

## Refactoring Needed

_(To be tracked during development)_

## Validation Results

_(To be populated during production readiness phase)_

## References

- [PRD: Multi-Host Daemon Selection](../../ft/web/1-WIP/PRD-2026-03-28-multi-host-daemon-selection.md)
- [Daemon multi_host.rs](../../../packages/tddy-daemon/src/multi_host.rs) — Existing foundation
- [connection.proto](../../../packages/tddy-service/proto/connection.proto) — Current proto
- [ConnectionScreen.tsx](../../../packages/tddy-web/src/components/ConnectionScreen.tsx) — Current web UI
- [tddy-livekit RpcClient](../../../packages/tddy-livekit/src/client.rs) — LiveKit RPC infrastructure
