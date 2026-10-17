# Changeset: LiveKit as a satellite — extension ports, local ConnectSession, ratchet (stack node 1)

**Date**: 2026-10-16
**Status**: 🚧 In Progress
**Type**: Bug Fix + Architecture Change
**Stack**: #satellite 1/11. Base: `master`. Branch: `satellite-1-extension-ports`, PR #623.

## Initial Discovery

[2026-10-16-livekit-satellite-ports-initial-discovery.md](./2026-10-16-livekit-satellite-ports-initial-discovery.md)
— five passes: live reproduction on Tddy Desktop, session-room coupling, LiveKit dependency
footprint, integration-status surface, request transport and deferred-work scan.

## Prerequisites

Scan run 2026-10-16 over `packages/*/docs/code-issues/` (19 hits) and `docs/dev/todo/` (67 hits).
No claimed code issue sits on this node's files (the only `**Claimed by:**` hit is in
`tddy-workflow-recipes`, unrelated).

| Item | Verdict | Note |
|---|---|---|
| [2026-09-10-the-dependency-boundary-harness-is-duplicated-per-crate.md](../todo/2026-09-10-the-dependency-boundary-harness-is-duplicated-per-crate.md) | ✅ RESOLVED HERE | The ratchet needs a workspace graph; one `cargo metadata` helper in `tddy-testing-commons` replaces both copies and fixes both blind spots (table-form and workspace-inherited deps). The two existing suites switch to it. |
| [2026-09-06-from-the-2026-09-06-optional-livekit-stack-evaluation.md](../todo/2026-09-06-from-the-2026-09-06-optional-livekit-stack-evaluation.md) — "is LiveKit configured?" decided in ≥6 places | ⚠ DURING — partly resolved here | The integration status board is the one runtime answer. Node 1 adds it and the LiveKit plugin reports into it; moving the six existing re-derivations onto it is nodes 2–5. Narrow the TODO at wrap, do not delete it. |
| `packages/tddy-daemon-kernel/docs/code-issues/heavy-dependency-livekit-peer-forwarding.md` | ℹ ANSWERED BY STACK — node 3 | Not touched here. Listed in the ratchet's known offenders. |
| [2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md](../todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md) | — unrelated to this node | A background room open can still wait on `join_room` (no deadline) — it no longer blocks a caller. `OpenSessionRoom` inherits today's `create_room` 30 s cap and uncapped `join_room`; recorded under Technical debt. |
| `packages/tddy-daemon-livekit/docs/code-issues/oversized-file-session-room.md` | ⚠ DURING | The reach implementation goes in a **new module** (`session_reach.rs`), not into `session_room.rs` (3,029 lines). |
| `packages/tddy-daemon-kernel/docs/code-issues/oversized-file-config.md` | ⚠ DURING | No change to `config.rs`: misconfiguration classification lives in the plugin. |

`tddy-daemon-extensions` is new and has no `docs/code-issues/` — unanalysed, not clean.

## Affected Packages

| Package | Change |
|---|---|
| **`tddy-daemon-extensions`** (new) | Port traits `SessionReach`, `ReachableSessions`; null object `NoSessionReach`; `IntegrationStatusBoard` / `IntegrationReporter` / `IntegrationState` |
| [`tddy-service`](../../../packages/tddy-service/README.md) | Proto: `session_room.proto` (`SessionRoomService.OpenSessionRoom`); `daemon_config.proto` `GetIntegrationStatus` |
| [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md) | `ConnectSession` reports reach through the port instead of opening the room |
| [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md) | `DaemonSessionHost::with_session_reach`; `impl ReachableSessions for DaemonSessionHost` |
| [`tddy-daemon-livekit`](../../../packages/tddy-daemon-livekit/README.md) | `session_reach` module: `LiveKitSessionReach` (implements `SessionReach`, owns `OpenSessionRoom`, reports status); `livekit_integration_state(Option<&LiveKitConfig>)` |
| [`tddy-daemon`](../../../packages/tddy-daemon/README.md) | Wires board + plugin; `DaemonConfigServiceImpl::with_integration_status`; registers the plugin's RPC entry |
| [`tddy-session-sync`](../../../packages/tddy-session-sync/README.md) | `open_session_room` replaces the `ConnectSession` call |
| [`tddy-terminal-rpc`](../../../packages/tddy-terminal-rpc/README.md) | `open_session_room_over_http` replaces `connect_session_over_http` |
| [`tddy-testing-commons`](../../../packages/tddy-testing-commons/README.md) | `dependency_graph::WorkspaceGraph` |
| `tddy-daemon-livekit`, `tddy-daemon-auth` (tests) | Boundary suites move onto the shared helper |

## Related Feature Documentation

- PRD: [PRD-2026-10-16-livekit-satellite-integration.md](../../ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-integration.md)
- [session-room.md](../../ft/daemon/session-room.md), [session-worktree-sync.md](../../ft/daemon/session-worktree-sync.md),
  [terminal-sessions.md](../../ft/daemon/terminal-sessions.md), [daemon-settings.md](../../ft/daemon/daemon-settings.md)

## Responsibility

The extension-port crate and the first port through it: a local session attach never depends on
LiveKit. Owns `tddy-daemon-extensions` (ports, null objects, status board), `SessionRoomService`,
`GetIntegrationStatus`, `LiveKitSessionReach`, the two `OpenSessionRoom` clients, `WorkspaceGraph`
and the ratchet test with its `KNOWN_OFFENDERS` list.

## Boundaries

- Moves no other LiveKit consumer behind a port; each later node owns its cluster (PRD § Implementation Plan).
- `ensure_session_room` stays for the agent-roster paths (node 11).
- No web change (node 9).
- Strikes no crate from `KNOWN_OFFENDERS`; it only creates the list.

## Dependencies

None: root node, based on `master`.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes. Every test drives this node's own surface.
**Concurrent with:** none
**Blocks:** every other node (each consumes `tddy-daemon-extensions` or edits the ratchet list)

    n1 → n2..n9      n2 → n10      n2, n3, n4, n10 → n11

## Draft PR contract

The draft publishes the **owned surface** and its failing tests; the implementation lands in this
same PR before it merges.

- `tddy-daemon-extensions`: `SessionReach`, `ReachableSessions`, `ReachedSession`, `NoSessionReach`
  (stub), `IntegrationStatusBoard` / `IntegrationReporter` / `IntegrationState` / `IntegrationStatus`
  (implemented — fixtures need it).
- Protos: `session_room.SessionRoomService.OpenSessionRoom`;
  `daemon_config.DaemonConfigService.GetIntegrationStatus`.
- `tddy-daemon-livekit::session_reach`: `LIVEKIT_INTEGRATION_ID`, `livekit_integration_state`,
  `LiveKitSessionReach::{new, open_now, session_room_entry}` + `SessionReach` impl.
- `DaemonSessionHost::with_session_reach`, `impl ReachableSessions for DaemonSessionHost`.
- `DaemonConfigServiceImpl::with_integration_status`.
- `tddy_session_sync::open_session_room`, `tddy_terminal_rpc::pty_relay::open_session_room_over_http`.
- `tddy_testing_commons::dependency_graph::WorkspaceGraph`.

Nodes 2–11 (PRD § Implementation Plan) code against these names.

## Summary

A local client's attach stops depending on LiveKit. `ConnectSession` reports that a session was
reached, with the host-stamped transport the request came in on, through a core port; the LiveKit
plugin opens the room synchronously for a caller that arrived over LiveKit and in the background for
everyone else. "Open the room or fail" moves to `SessionRoomService.OpenSessionRoom`, which the
plugin registers and the two in-tree callers adopt. An integration status board makes the plugin's
state readable through `DaemonConfigService.GetIntegrationStatus`. A workspace ratchet pins which
crates may name a LiveKit crate and lists the 18 that still do.

## Background

See PRD § Background. Reproduced 2026-10-16: desktop with `livekit:` pointing at a server whose
port 7880 times out; two `claude-cli` sessions started (agents alive), neither terminal attached.

## Scope

- [ ] `tddy-daemon-extensions` crate: ports, null object, status board
- [ ] Protos: `SessionRoomService.OpenSessionRoom`, `DaemonConfigService.GetIntegrationStatus`
- [ ] `ConnectSession` via `SessionReach`; never awaits LiveKit for a non-LiveKit caller
- [ ] `LiveKitSessionReach`: sync for `RequestTransport::LiveKit`, background otherwise; `OpenSessionRoom`; status reporting
- [ ] `tddy-session-sync` and `pty-relay` call `OpenSessionRoom`
- [ ] Daemon wiring (board, plugin, RPC entry, `GetIntegrationStatus`)
- [ ] `WorkspaceGraph` helper; boundary suites migrated; ratchet test

## Technical Changes

### State A

- `connect_session_at_session_coordinate` (`tddy-agent-launch/src/session_coordinate_handlers.rs:105-189`)
  calls `self.agent_roster.host.ensure_session_room(..).await?` for `claude-cli`/`cursor-cli` with a
  recorded `repo_path`, then returns empty coordinates for `claude-cli | cursor-cli | workspace`.
- `DaemonSessionHost::ensure_session_room` → `SessionRoomRegistry::ensure_open` (single-flight per
  session; `open_measured_by` → `create_room` (30 s) → `join_room` (no deadline); then
  `SessionTerminalBridge::bridge`). `Ok(None)` without credentials.
- Session-sync (`attach.rs:263`) and pty-relay (`pty_relay.rs:279`) call `ConnectSession` over HTTP
  for its side effect.
- No queryable LiveKit status; failures are log lines.
- 27 workspace crates name a LiveKit crate in normal/build/optional dependencies; no test constrains
  it.

### State B

**New crate `tddy-daemon-extensions`** — depends on `tddy-rpc`, `async-trait`, `tokio` (sync), and
nothing that reaches LiveKit.

```rust
// session_reach.rs
pub struct ReachedSession {
    pub session_id: String,
    pub session_dir: PathBuf,
    pub worktree_root: PathBuf,
    pub arrived_over: tddy_rpc::RequestTransport,
}

/// The host side a reach extension calls back into — implemented once, by the session host.
#[async_trait]
pub trait ReachableSessions: Send + Sync {
    /// Authenticate and resolve a session this host runs, for an extension's own RPC.
    async fn resolve(&self, session_token: &str, session_id: &str) -> Result<ReachedSession, Status>;
    /// The RPC surface a remote participant reaches this session through.
    fn session_services(&self) -> Result<tddy_rpc::MultiRpcService, Status>;
    /// Make the session's terminal drivable by a remote participant.
    async fn expose_terminal(&self, session_id: &str) -> Result<(), Status>;
}

/// Something that extends a session's reach beyond this host.
#[async_trait]
pub trait SessionReach: Send + Sync {
    async fn session_reached(&self, session: ReachedSession, host: Arc<dyn ReachableSessions>)
        -> Result<(), Status>;
}

/// No extension wired: a session is reachable on this host and nowhere else.
pub struct NoSessionReach;

// integration_status.rs
pub enum IntegrationState { Disabled, Misconfigured { missing: Vec<String> }, Connecting,
                            Connected, Unreachable { error: String } }
pub struct IntegrationStatus { pub id: String, pub label: String, pub state: IntegrationState,
                               pub since: SystemTime }
pub struct IntegrationStatusBoard { .. }       // Default; Arc-shared
impl IntegrationStatusBoard {
    pub fn reporter(self: &Arc<Self>, id: &str, label: &str) -> IntegrationReporter; // registers as Disabled
    pub fn snapshot(&self) -> Vec<IntegrationStatus>;                                 // sorted by id
}
pub struct IntegrationReporter { .. }          // Clone
impl IntegrationReporter { pub fn report(&self, state: IntegrationState); }
```

**`tddy-agent-launch`** — `LaunchSessions` gains `session_reach: Arc<dyn SessionReach>` (default
`NoSessionReach`) and the host as `Arc<dyn ReachableSessions>`. `ConnectSession` resolves the
session as today, then for `session_type_is_facilitated_here` with a `repo_path` calls
`session_reached(ReachedSession { .., arrived_over: request.metadata().transport() }, host)` and
propagates its result. The port decides whether that waits.

**`tddy-session-lifecycle`** — `DaemonSessionHost::with_session_reach(Arc<dyn SessionReach>)`
(`#[must_use]`, before `with_rpc_families`); `impl ReachableSessions for DaemonSessionHost`
(`resolve` reuses the connect handler's token→user→session-dir path; `session_services` =
`session_room_roster`; `expose_terminal` = today's `SessionTerminalBridge::bridge`).
`ensure_session_room` remains for the agent-roster paths (B–G in discovery) — node 8 moves those.

**`tddy-daemon-livekit`** — new module `session_reach`:

```rust
pub const LIVEKIT_INTEGRATION_ID: &str = "livekit";

/// What the `livekit:` block alone says: Disabled (absent / enabled: false and no credentials),
/// Misconfigured { missing } (any of url, api_key, api_secret blank), else Connecting.
pub fn livekit_integration_state(livekit: Option<&LiveKitConfig>) -> IntegrationState;

pub struct LiveKitSessionReach { .. }
impl LiveKitSessionReach {
    pub fn new(config: DaemonConfig, rooms: Arc<SessionRoomRegistry>, status: IntegrationReporter) -> Arc<Self>;
    /// Open (or reuse) the room and expose the terminal now; fails naming the room.
    pub async fn open_now(&self, session: &ReachedSession, host: Arc<dyn ReachableSessions>)
        -> Result<Option<OpenedSessionRoom>, Status>;
    /// The `session_room.SessionRoomService` entry, answering through `sessions`.
    pub fn session_room_entry(self: &Arc<Self>, sessions: Arc<dyn ReachableSessions>) -> tddy_rpc::ServiceEntry;
}
impl SessionReach for LiveKitSessionReach {
    // RequestTransport::LiveKit → open_now(..).await.map(drop)
    // otherwise → tokio::spawn(open_now(..)); Ok(())
}
```

Every `open_now` outcome reports: `Ok(Some)` → `Connected`; `Err` → `Unreachable { error }`;
`Ok(None)` → the config's state (`Disabled`/`Misconfigured`). The common-room discovery loop reports
`Connecting`/`Connected`/`Unreachable` on the same reporter.

**Protos (`tddy-service`)**

```proto
// session_room.proto
package session_room;
service SessionRoomService {
  // Open the session's LiveKit room (or reuse it) with this daemon in it and the session's
  // terminal exposed. Fails, naming the room, when LiveKit cannot be reached.
  rpc OpenSessionRoom(OpenSessionRoomRequest) returns (OpenSessionRoomResponse);
}
message OpenSessionRoomRequest { string session_token = 1; string session_id = 2; }
message OpenSessionRoomResponse { string livekit_room = 1; string livekit_url = 2; string livekit_server_identity = 3; }

// daemon_config.proto (additions)
rpc GetIntegrationStatus(GetIntegrationStatusRequest) returns (GetIntegrationStatusResponse);
message GetIntegrationStatusRequest { string session_token = 1; }
message GetIntegrationStatusResponse { repeated IntegrationStatus integrations = 1; }
enum IntegrationState { INTEGRATION_STATE_UNSPECIFIED = 0; INTEGRATION_STATE_DISABLED = 1;
  INTEGRATION_STATE_MISCONFIGURED = 2; INTEGRATION_STATE_CONNECTING = 3;
  INTEGRATION_STATE_CONNECTED = 4; INTEGRATION_STATE_UNREACHABLE = 5; }
message IntegrationStatus { string id = 1; string label = 2; IntegrationState state = 3;
  string error = 4; repeated string missing_fields = 5; int64 since_unix_ms = 6; }
```

**`tddy-daemon`** — `runtime.rs` builds one `IntegrationStatusBoard`, one `LiveKitSessionReach`
(reporter `livekit`), `host.with_session_reach(reach)`, registers `reach.session_room_entry(host)`
alongside the daemon's entries; `DaemonConfigServiceImpl::with_integration_status(board)` serves
`GetIntegrationStatus` (token-gated like `GetConfig`; `failed_precondition` when unwired).

**Clients** — `tddy_session_sync::open_session_room(daemon_url, session_token, session_id, timeout)
-> Result<OpenSessionRoomResponse, DaemonHttpError>` (pub; `attach` calls it);
`tddy_terminal_rpc::pty_relay::open_session_room_over_http(daemon_url, session_token, session_id)`
(pub, `livekit` feature; replaces `connect_session_over_http`).

**`tddy-testing-commons::dependency_graph`**

```rust
pub struct WorkspaceGraph { .. }
impl WorkspaceGraph {
    pub fn load() -> Self;                                         // `cargo metadata --no-deps`
    pub fn workspace_crates(&self) -> BTreeSet<String>;
    pub fn names_directly(&self, krate: &str) -> BTreeSet<String>; // normal + build + optional; not dev
    pub fn reaches(&self, krate: &str) -> BTreeSet<String>;        // transitive over the above
}
```

### Delta

| Package | Files |
|---|---|
| `tddy-daemon-extensions` (new) | `Cargo.toml`, `src/lib.rs`, `src/session_reach.rs`, `src/integration_status.rs`, `tests/` |
| `tddy-service` | `proto/session_room.proto` (new), `proto/daemon_config.proto`, `build.rs`, `src/lib.rs` (module) |
| `tddy-agent-launch` | `launch_ports.rs` (field), `session_coordinate_handlers.rs` |
| `tddy-session-lifecycle` | `connection_service.rs` (field), `svc_host_builders.rs` (`with_session_reach`), new `svc_reachable_sessions.rs`, `handler_state.rs`; `tests/session_room_acceptance.rs` |
| `tddy-daemon-livekit` | new `session_reach.rs`, `lib.rs`; `livekit_peer_discovery.rs` (reporter hook); `tests/livekit_integration_status.rs` |
| `tddy-daemon` | `runtime.rs`, `daemon_config_service.rs`; `tests/daemon_config_service.rs` |
| `tddy-session-sync` | `attach.rs`; `tests/attach_acceptance.rs` |
| `tddy-terminal-rpc` | `pty_relay.rs`; `tests/` |
| `tddy-testing-commons` | `dependency_graph.rs`, `lib.rs`; existing boundary suites in `tddy-daemon-livekit`, `tddy-daemon-auth` |
| `tddy-rpc` | `Cargo.toml`: tokio `rt` feature |
| root `Cargo.toml` | workspace member |

## Implementation Milestones

- [x] M1 Extensions crate compiles with ports + board; its own unit tests green
- [x] M2 Protos generated; stubs registered
- [x] M3 `ConnectSession` through `SessionReach`; local-connect acceptance green
- [x] M4 `LiveKitSessionReach` + `OpenSessionRoom`; session-room acceptance (testkit) green
- [x] M5 Status reporting + `GetIntegrationStatus` (the common-room discovery loop's reporter hook is open — see Technical Debt)
- [x] M6 Clients switched; their tests green
- [x] M7 `WorkspaceGraph`; boundary suites migrated; ratchet green

## Testing Plan

**Levels.** The defect is an RPC-level contract on `DaemonSessionHost`, so the acceptance tests drive
`ConnectSession` / `OpenSessionRoom` on a real host with a real (or deliberately unreachable)
LiveKit — extending `session_room_acceptance.rs`'s `FacilitatingDaemon`, which already has
`ALiveKitNobodyAnswers` and the nothing-listens address. Status classification and the board are
pure → unit. Clients are pinned at their HTTP boundary with `stub_http_routed` (which 404s any path
not routed, so a stray `ConnectSession` fails). The ratchet is a workspace-graph test.

**Fixture change.** `FacilitatingDaemon::connect_to` keeps its meaning — "as a client reaching it
over LiveKit does" — and therefore stamps `RequestTransport::LiveKit` instead of `Request::direct`.
The fixture wires `LiveKitSessionReach` and registers its `SessionRoomService` entry, exactly as the
daemon will. New `connect_locally_to(id)` stamps `RequestTransport::InProcess`.

**The reversed test.** `connecting_to_a_session_fails_when_livekit_cannot_be_reached` is replaced
by `connecting_over_livekit_fails_when_livekit_cannot_be_reached` (same assertion, explicit
transport) plus the local tests below. The old test's *name* states the defect as the contract; it
is deleted, not weakened.

### Acceptance tests

**`packages/tddy-session-lifecycle/tests/session_room_acceptance.rs`** (existing suite)

| Test | Validates |
|---|---|
| `connecting_locally_answers_without_waiting_on_a_livekit_that_never_answers` | In-process `ConnectSession` returns within 2 s with LiveKit accepting and never answering — the reported defect |
| `connecting_locally_succeeds_when_nothing_listens_where_livekit_is_configured` | Same with `ECONNREFUSED`: `Ok`, empty coordinates |
| `connecting_over_livekit_fails_when_livekit_cannot_be_reached` | Remote caller still hears the failure, naming `session-{id}` (**passes today** — pins what must survive) |
| `opening_a_session_room_fails_when_livekit_cannot_be_reached` | `OpenSessionRoom` is loud, naming the room |
| `a_room_that_could_not_be_opened_after_a_local_connect_is_reported_unreachable` | Background failure lands on the board as `Unreachable`, naming the room |
| `a_host_without_the_livekit_plugin_never_contacts_livekit_on_connect` | Host built without the plugin, LiveKit configured at nobody-answers: local and LiveKit-stamped connects both answer, zero connections |
| `opening_a_session_room_names_the_room_and_the_daemon_waiting_in_it` | Testkit: response = `session-{id}`, configured URL, `daemon-{instance}`; probe sees it |
| `a_session_connected_locally_becomes_drivable_over_livekit_without_another_call` | Testkit: background open bridges the terminal into the lobby; room on server; terminal drivable; board `Connected` |

Fixture: `connect_to` now stamps `RequestTransport::LiveKit`; `connect_locally_to` stamps `InProcess`;
`open_session_room` calls the plugin's entry over `Http`; `ThePlugin::{Wired, NotWired}` selects the
wiring. The reversed test `connecting_to_a_session_fails_when_livekit_cannot_be_reached` is deleted.

**`packages/tddy-daemon-livekit/tests/livekit_integration_status.rs`** (new)

| Test | Validates |
|---|---|
| `a_room_that_cannot_be_opened_reports_livekit_unreachable_with_the_failure` | `open_now` against nothing-listens → board `Unreachable { error }` equal to the returned `Status` message |
| `a_daemon_with_no_livekit_block_opens_no_room_and_reports_livekit_disabled` | `Ok(None)`, board `Disabled` |
| `a_room_opened_on_a_reachable_server_reports_livekit_connected` | Testkit (`#[serial(livekit_docker)]`): room = `session-{id}`, board `Connected` |

**`packages/tddy-daemon/tests/daemon_config_service.rs`** (existing suite)

| Test | Validates |
|---|---|
| `reports_each_integration_the_board_holds_with_its_state_and_failure` | Two integrations, ordered by id; `UNREACHABLE` + error; `DISABLED`; `since_unix_ms` from the board |
| `reports_the_fields_a_misconfigured_integration_is_missing` | `MISCONFIGURED` + `missing_fields` |
| `refuses_integration_status_to_a_caller_without_a_valid_session_token` | `Unauthenticated`, like `GetConfig` |

**Clients**

| File | Test | Validates |
|---|---|---|
| `packages/tddy-session-sync/tests/attach_acceptance.rs` | `asks_the_daemon_to_open_the_session_room_through_the_room_service` | `open_session_room` posts `OpenSessionRoomRequest` to `/rpc/session_room.SessionRoomService/OpenSessionRoom` and nothing else |
| same | `reports_a_daemon_without_the_room_service_as_refusing_open_session_room` | 404 → `Refused { method: "OpenSessionRoom", code: "404" }` |
| `packages/tddy-terminal-rpc/tests/pty_relay_room_opening.rs` (new, `livekit` feature) | `asks_the_daemon_to_open_the_session_room_before_joining_it` | `open_session_room_over_http` hits only the room service path with the request body |

**Ratchet — `packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs`** (new)

| Test | Validates |
|---|---|
| `the_extension_ports_reach_no_livekit_crate` | `reaches("tddy-daemon-extensions") ∩ LIVEKIT_FAMILY = ∅` |
| `only_plugins_wiring_and_known_offenders_name_a_livekit_crate` | Every crate naming a family crate is in `PLUGINS ∪ WIRING ∪ KNOWN_OFFENDERS` |
| `every_known_offender_still_names_a_livekit_crate` | A crate that stopped must be struck from the list (ratchet tightens) |

### Unit tests

| File | Test | State after red |
|---|---|---|
| `tddy-daemon-extensions/src/integration_status.rs` | `an_integration_is_disabled_until_it_reports_otherwise`, `the_latest_report_is_the_integration_state`, `reporting_the_state_it_is_already_in_keeps_when_it_entered_it`, `a_change_of_state_moves_when_it_began`, `integrations_are_listed_by_id_and_report_independently` | ✅ pass — board implemented early (see Technical Debt) |
| `tddy-daemon-extensions/src/session_reach.rs` | `with_nothing_wired_a_reach_from_anywhere_succeeds_without_asking_the_host` | ❌ `todo!()` |
| `tddy-daemon-livekit/src/session_reach.rs` | `a_daemon_with_no_livekit_block_has_livekit_disabled`, `a_switched_off_block_with_no_credentials_is_disabled_not_misconfigured`, `a_block_missing_credentials_is_misconfigured_naming_them_in_order` (3 cases), `a_complete_block_is_connecting_until_the_server_answers` | ❌ `todo!()` |
| `tddy-testing-commons/src/dependency_graph.rs` | `lists_every_member_of_the_workspace`, `names_inline_sub_table_optional_and_build_dependencies_but_not_dev_ones`, `names_a_dependency_inherited_from_the_workspace`, `reaches_every_hop_but_not_the_crate_itself_nor_its_dev_dependencies` | ❌ `todo!()` |

## Acceptance Tests

See Testing Plan tables (each row = one test, file path given).

## Technical Debt & Production Readiness

- **Red-phase deviations (2026-10-16).** `IntegrationStatusBoard` / `IntegrationReporter` were
  implemented during the acceptance step, not left as `todo!()`: every `FacilitatingDaemon` builds a
  reporter, so a stubbed board failed the *whole* suite — including tests that pass today — for a
  reason unrelated to their subject. Its unit tests (Step 7) pin it rather than drive it.
  `LiveKitSessionReach::new` and `session_room_entry` are real for the same reason; their behaviour
  (`open_now`, `session_reached`, `OpenSessionRoom`) is `todo!()`.
- **`tddy-rpc`'s tokio dependency lacked `rt`** although it calls `tokio::spawn`; it compiled only
  through feature unification and failed when built alone (first time a crate depended on
  `tddy-rpc` with no other tokio user). Fixed in its manifest.

- **TODO: the common-room discovery loop does not yet report on the board.** `LiveKitSessionReach`
  reports `Connecting` (from the `livekit:` block, at construction), `Connected` and `Unreachable`
  (from every room open); `spawn_common_room_discovery_loop` has no reporter, because taking one
  changes a signature four test suites call. Needs its own decision on how the hook is passed.
- `ensure_session_room` and the agent-roster paths are untouched (node 11 per Boundaries).
- The two boundary suites no longer read `[dev-dependencies]` of their own crate: `WorkspaceGraph`
  names normal, build and optional dependencies only (its contract). The test that asserted
  "declares the daemon in any section" is now `does_not_declare_the_daemon_as_a_dependency_of_its_own`.
- `join_room` still has no deadline; a background open against a hanging server holds that
  session's opening lock until LiveKit gives up. A subsequent `OpenSessionRoom` for the same session
  waits behind it.
- `CliSessionManager::ensure_livekit_terminal` holds the all-sessions write lock across a LiveKit
  connect (discovery § 2). Not on a local caller's path after this change; node 9 owns it.

## Decisions & Trade-offs

- **Plugin-owned RPC over a request flag** (developer, 2026-10-16): the core proto never learns
  about rooms.
- **The transport decides sync vs background, inside the plugin**: the core passes the
  host-stamped `RequestTransport` and decides nothing. A remote web client needs no client change.
- **`Direct` counts as local**: it is in-process code, never a remote reach.
- **Ports in a new crate** (developer): not the kernel, which reaches LiveKit until node 3 and is
  already oversized.
- **Ratchet on direct naming, transitive only for the extensions crate**: transitive closure of
  every library is not closable until node 3; the direct list is what each node can shrink.
- **`tddy-daemon` is wiring** although its lib holds services (`daemon_config_service`,
  `credential_sync`): it is the daemon's composition root; the desktop embeds it.
- **Allowed set**: plugins `tddy-livekit`, `tddy-livekit-testkit`, `tddy-daemon-livekit`,
  `tddy-livekit-screen-capture`, `tddy-screenshare`; wiring `tddy-daemon`, `tddy-rdp`, `tddy-vnc`;
  test harness `tddy-e2e`. Client lib+bin crates (`tddy-coder`, `tddy-session-sync`,
  `tddy-remote-git-repo`) are offenders until node 10 (developer: "plugin + daemon wiring only").
- **Known offenders (18)**: `tddy-agent-launch`, `tddy-cli-sessions`, `tddy-coder`,
  `tddy-daemon-auth`, `tddy-daemon-kernel`, `tddy-daemon-rpc`, `tddy-host-service`,
  `tddy-remote-git-repo`, `tddy-screen-sharing`, `tddy-session-activity`, `tddy-session-agents`,
  `tddy-session-files`, `tddy-session-lifecycle`, `tddy-session-split`, `tddy-session-sync`,
  `tddy-session-tool-client`, `tddy-telegram-control`, `tddy-terminal-rpc` — computed from
  `cargo metadata` 2026-10-16.

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

- [x] Record initial discovery (`2026-10-16-livekit-satellite-ports-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (approved 2026-10-16)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes the discovery file
- [ ] USER REVIEW — work complete, decide next steps
