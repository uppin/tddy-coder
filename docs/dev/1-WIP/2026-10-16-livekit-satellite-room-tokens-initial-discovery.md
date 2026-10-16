# Initial Discovery: #satellite 4/11 — room-tokens

**Changeset**: [2026-10-16-livekit-satellite-room-tokens.md](./2026-10-16-livekit-satellite-room-tokens.md)
**Date**: 2026-10-16
**Passes**: 2

## Combined Conclusions

- `tddy_service::TokenProvider` (`token_service.rs:13-18`) already decouples the token *service* from LiveKit, but it has a fixed TTL and `String` errors; daemon minting needs per-call TTLs (`DEFAULT_LIVEKIT_JWT_TTL_SECS`, `MINTED_ROOM_TOKEN_TTL` = 3600 s at `auth.rs:437`) and `Status` errors → a new `RoomTokenMinter` port, adapted onto `TokenProvider` in daemon-auth.
- Mint sites outside the plugin: daemon-auth `auth.rs:330` (token service), `:523` (`LiveKitTokenServiceImpl`); screen-sharing `screen_sharing_service.rs:503` (`build_bridge_spawn_config`, private fn at `:477`); plus cli-sessions, tddy-coder (4×), session-sync, terminal-rpc, session-agents, session-split, livekit-screen-capture — each owned by another node or a plugin crate.
- `tddy-screen-sharing` names `tddy-livekit` only for this mint → strikable here. `tddy-daemon-auth` keeps `livekit::Room` / `tddy_livekit::RpcClient` in `oauth_loopback_tunnel.rs` → not strikable until node 10.
- TODO `2026-08-13-tokengenerator-generate-for-performs-no-authorization.md` applies to the port too (it authorizes nothing).

## Exploration 1 — whole-work discovery (copied from #satellite 1/11, #623)

**Changeset**: [2026-10-16-livekit-satellite-ports.md](./2026-10-16-livekit-satellite-ports.md)
**Date**: 2026-10-16
**Passes**: 5

### Combined Conclusions

1. **The trigger.** `ConnectSession` for a `claude-cli` / `cursor-cli` session calls
   `self.agent_roster.host.ensure_session_room(..).await?`
   (`packages/tddy-agent-launch/src/session_coordinate_handlers.rs:137-155`) **before** the early
   return that hands a local session its empty coordinates (`:157-166`). With LiveKit configured
   and unreachable, `create_room` (30 s cap) / `join_room` (no cap) fail, `ConnectSession` fails,
   the web's `useSessionAttachment` goes to `error`, and the pane shows "Select Resume to
   reconnect". The local desktop reaches its own host over IPC and never reads the room hint
   (`packages/tddy-web/src/rpc/connections/localHost.ts:327`) — the room was never needed for it.
   Reproduced live 2026-10-16 against `ws://raitininkai.ddnsgeek.com:7880` (port 7880 times out).
2. **The fail-loud contract was deliberate** (#451, `docs/dev/changesets/2026-09-06-lazy-session-room.md`):
   `ConnectSession` is both what puts `daemon-{instance_id}` in the room and what tells a caller
   which room to look in; `tddy-session-sync` and `tddy-tools pty-relay --livekit-url` call it
   before joining. A test pins it:
   `connecting_to_a_session_fails_when_livekit_cannot_be_reached`
   (`packages/tddy-session-lifecycle/tests/session_room_acceptance.rs:1340`). This change reverses
   it **by moving the fail-loud contract to a plugin-owned RPC**, not by swallowing the error.
3. **Request transport is already host-stamped.** `tddy_rpc::RequestTransport`
   (`packages/tddy-rpc/src/message.rs:13`) — `InProcess | LiveKit | UnixSocket | Pipe | Http |
   Grpc | Direct` — readable as `request.metadata().transport()` and already used for a security
   decision (`packages/tddy-daemon-auth/src/first_login_admission.rs:98`). A caller that arrived
   **over LiveKit** is a remote client that needs the room and bridge now; anything else is local.
4. **LiveKit reaches 37 crates through normal dependencies.** The single largest lever is
   `tddy-daemon-kernel::peer_forwarding` (sole SDK consumer in the kernel; ~20 dependents inherit
   the SDK) — open code issue
   `packages/tddy-daemon-kernel/docs/code-issues/heavy-dependency-livekit-peer-forwarding.md`.
   `tddy-daemon-livekit` holds three LiveKit-free modules core crates import (`placement`,
   `os_user_resolution`, `session_git`) plus re-exports of kernel identity helpers.
   `tddy-daemon-rpc` and `tddy-session-lifecycle` declare `livekit = "0.7"` with no `livekit::`
   use in `src/`.
5. **No queryable LiveKit status exists.** Only log lines; `SupervisedCommonRoom` keeps the
   *desired* target in a `watch` channel, never the outcome. The RPC participant
   (`tddy-livekit/src/participant.rs:473`) logs a failed join and never retries.
   `DaemonConfigService` (served over HTTP/IPC, so reachable when LiveKit is down) is the home for
   a status read. Side defect: an `api_key`/`api_secret`-only edit is never applied to the running
   common room (`packages/tddy-daemon/src/daemon_settings.rs:124-133`).
6. **House port styles exist** and the new ports follow them: null-object named port
   (`WorktreeRoomCloser` / `NoSessionRooms`, `packages/tddy-worktree-service/src/service.rs:36-56`),
   optional observer with `with_*` builder (`SessionWorktreeObserver`,
   `packages/tddy-agent-launch/src/session_worktree_observer.rs:17-43`), required port that refuses
   when unwired (`DaemonRpcFamilies`, `packages/tddy-session-lifecycle/src/rpc_families.rs`).
   Ports owned by the LiveKit crate today: `SessionTerminalBridge`, `WorktreeSource`,
   `SessionTokenMinter`, `RemoteSnapshotSource` (`tddy-daemon-livekit/src/session_room.rs`).
7. **Dependency-boundary harness** exists twice (`tddy-daemon-livekit/tests/dependency_boundary_unit.rs`,
   `tddy-daemon-auth/tests/dependency_boundary_unit.rs`, ~100 identical lines, two known blind
   spots: table-form deps and workspace-inherited deps) and a `cargo metadata` variant
   (`tddy-daemon-sandbox/tests/sandbox_app_dependency_reverses.rs`). TODO
   `docs/dev/todo/2026-09-10-the-dependency-boundary-harness-is-duplicated-per-crate.md` asks to
   lift it into `tddy-testing-commons`.

### Exploration 1 — reproduction from the running desktop (parent, Bash)

Sequence:
1. `ps aux | grep tddy-…` — desktop pid 85561 hosting the daemon; claude pids 9772
   (`01a145a9…`, fix-livekit-secret) and 21940 (`01a145ac…`, fix-agent-terminal) alive.
2. `~/.tddy/desktop.yaml` — `livekit: { enabled: true, url: ws://raitininkai.ddnsgeek.com:7880,
   api_key: devkey, api_secret: secret, common_room: tddy-lobby }`; mtime 19:59:44, after app
   start 19:56:54.
3. `~/.tddy/logs/daemon` — `20:04:33 started claude-cli session 01a145ac… pid=21940`, then
   `ConnectSession` dispatches at 20:04:33 ×2, 20:04:39, 20:04:43, 20:04:44, no error logged;
   `common_room_discovery_cycle ended: … validate request timed out` every ~34 s.
4. `~/.tddy/logs/webrtc` — `signal connection failed on v0 path: Timeout`.
5. `nc -z -G 3 raitininkai.ddnsgeek.com 7880` → fail; `7881` → fail; `443` → succeeds; DNS
   `5.20.229.130`. Previous log file (`daemon.2026-10-15T23-06-16`) has zero `livekit` mentions.
6. Read `packages/tddy-web/src/components/sessions/SessionMainPane.tsx:380-470` — placeholder
   rendered when `!hasRuntimes && overlay === null && !isConnected`.
7. Read `useSessionAttachment.ts:60-160`, `rpc/connections/sessionAttachment.ts:1-90`,
   `localHost.ts:300-400` — local host ignores the hint's LiveKit fields by design.
8. Read `SessionsDrawerScreen.tsx:425-660` — attach on selection via `connectSession`; error →
   inspector opens.
9. Read `session_coordinate_handlers.rs:95-200`, `session_room_opening.rs`,
   `session_room.rs:1482-1611` (`open_measured_by`, `ensure_open`).
10. `git log -L` on the handler — last touched by #535/#536 (`#carve` 20–21, moves only); the
    room-opening line dates from #451.

### Exploration 2 — session-room coupling (Explore agent)

Grep sequence: `ensure_session_room|ensure_open\b|open_measured_by|session_rooms|SessionRooms|create_room|join_room`;
`AgentHostCallbacks`; `ensure_session_room_for_agents|claim_agent_clone|claim_co_located_seed_clones|seed_session_agent_roster|require_opened_session_room`;
pub surface of `session_room.rs`; `SessionTerminalBridge|LiveKitCredentials|close_session_room`;
`pub struct DaemonSessionHost`; `livekit` in `tddy-daemon/src/runtime.rs` and desktop;
`ensure_livekit_terminal|livekit_creds_from_config`; `connect_session|ConnectSessionRequest` in tests;
port-pattern grep over session-lifecycle/agent-launch/daemon-rpc/session-agents/session-split/worktree-service;
implementors of the host traits; `spawn_livekit_bridge|timeout|connect` in `livekit_bridge.rs`.

Room-opening call sites (contact LiveKit):

| # | Location | Path | Handling | Local critical path? |
|---|---|---|---|---|
| A | `tddy-agent-launch/src/session_coordinate_handlers.rs:137-155` | ConnectSession → `agent_roster.host.ensure_session_room` | `?` | **yes — the bug** |
| B | `tddy-session-agents/src/svc_ensure_session_room_for_agents.rs:37-41` | agent from another daemon | `?` + `require_opened_session_room` | no |
| C | same `:60` | `claim_agent_clone` → B | `?` | no |
| D | same `:209`, `:284` | seed/claim for **remote** records only | `?` | no |
| E | `svc_start_sandboxed_{claude,cursor}_cli_session.rs` | `claim_co_located_seed_clones` | `?` | only with a remote seeded agent |
| F | `workspace_branch_start.rs:64-67` | `seed_session_agent_roster` | `?` | remote seeded only |
| G | `svc_session_agent_port_adapters.rs:68-91` | AttachSessionAgent, remote agent | `?` | no |
| H | `tddy-session-split/src/svc_spawn_split_agent.rs:280-337` | split start | `?` | split placement only (LiveKit is its transport) |
| I | `tddy-session-lifecycle/.../session_room_opening.rs:20-40` | impl behind A–G | | |
| J | `tddy-daemon-livekit/src/session_room.rs:1446/1482/1517/1518` | `open`/`open_measured_by`/`create_room`/`join_room` | `?` | |

Resume, signal, delete: no LiveKit contact for claude-cli/cursor-cli/workspace
(`svc_resume_session.rs:53-107`; start core `svc_start_session_core.rs:350,389`).

Trait verbatim (`tddy-session-agents/src/agent_host_callbacks.rs:36-80`):

```rust
#[async_trait::async_trait]
pub trait AgentHostCallbacks: Send + Sync {
    async fn worktree_snapshot(&self, session_token: &str, codebase_session_id: &str, codebase_instance_id: &str)
        -> Result<WorktreeSnapshot, Status>;
    async fn run_exec_tool_locally(&self, req: &ExecuteToolRequest, sessions_base: &Path, worktree_root: &Path)
        -> ExecuteToolResponse;
    fn hosted_clone_for(&self, session_id: &str) -> Option<Arc<HostedClone>>;
    async fn run_hosted_clone_tool(&self, req: &ExecuteToolRequest, clone: &HostedClone) -> ExecuteToolResponse;
    async fn ensure_session_room(&self, session_id: &str, session_dir: &Path, worktree_root: &Path)
        -> Result<Option<OpenedSessionRoom>, Status>;
}
```

One implementor (`DaemonSessionHost`, `svc_agent_host_ports.rs:30-78`), no doubles.
`SplitHost: AgentHostCallbacks` and `LaunchHost` likewise implemented only on `DaemonSessionHost`.

`ensure_open` (`session_room.rs:1551-1585`): per-session `opening_lock`; reuse when
`already_open`; else `open_measured_by(..).await?`; then `terminal.bridge(id).await?` when a room
exists. `open_measured_by` returns `Ok(None)` when `LiveKitCredentials::from_config` is `None`
(url/api_key/api_secret all non-blank required — **`enabled` is not consulted**).

`CliSessionManager::ensure_livekit_terminal`
(`tddy-cli-sessions/src/cli_session_manager/livekit_terminals.rs:61-101`) holds the
`livekit_terminals` write lock across `spawn_livekit_bridge` → `LiveKitParticipant::connect` (no
timeout) — one hung connect blocks every session's bridge.

`DaemonSessionHost` (`tddy-session-lifecycle/src/connection_service.rs:85-205`) fields include
`room_roster: Arc<dyn RoomRoster>`, `session_rooms: Arc<SessionRoomRegistry>`,
`session_admissions`, `rpc_families: Option<Arc<dyn DaemonRpcFamilies>>`,
`worktree_observer: Option<Arc<dyn SessionWorktreeObserver>>`. Constructor
`new(config, sessions_base_for_user, tddy_data_dir, user_resolver, spawn_client, livekit_discovery, presenter_event_sink, claude_cli_manager)`
(`svc_host_builders.rs:28-38`). Daemon wiring `tddy-daemon/src/runtime.rs:768` (supervisor),
`:781-785` (shared `SessionRoomRegistry`), `:1125-1145` (host build), `:1146-1156`
(`with_worktree_observer` — optional-port precedent). Desktop embeds `runtime::build` with no
LiveKit wiring of its own (`tddy-desktop/src-tauri/src/lib.rs:219-282`).

Test harness — `tddy-session-lifecycle/tests/session_room_acceptance.rs`, fixture
`FacilitatingDaemon` (`:300-411`): `with_livekit`, `without_livekit`,
`with_livekit_that_never_answers(&ALiveKitNobodyAnswers)` (`:1138`),
`with_livekit_nothing_listens_on()` (`:1201`), `a_session_being_connected_to`, `connect_to(id) ->
Result<ConnectSessionResponse, Status>`, `room_on_the_server`, `lobby_participants`,
`drives_the_terminal_over_livekit`. Helpers `a_livekit_nobody_answers()` (`:1094-1135`, accepts
and never answers, counts connections), `a_livekit_address_nothing_listens_on()` (`:1188-1197`),
`ALiveKitThatComesBack` (`:1702-1745`). Tests: `:1156 starting_a_session_dials_livekit_not_at_all`,
`:1340 connecting_to_a_session_fails_when_livekit_cannot_be_reached` (**pins the old contract**),
`:1769 a_session_started_while_livekit_was_down_is_drivable_over_livekit_once_it_is_back`, and
testkit-backed connects at `:1265, 1298, 1508, 1540, 1581, 1605, 1635` (`#[serial]`).
`tddy-session-lifecycle/src/test_util.rs`: `test_config`, `test_host`, `TestDaemon`,
`RpcFamiliesNotUnderTest`. No `tests/` dir in `tddy-agent-launch`.

Port patterns (verbatim excerpts recorded in the agent dump): `SessionWorktreeObserver` +
`with_worktree_observer`; `WorktreeRoomCloser` + `NoSessionRooms` ("Named rather than an
`Option<…>` … so the 'nothing to close' case is a decision somebody wrote down"); `DaemonRpcFamilies`
+ `rpc_families()` refusing when unwired; `SessionAgentPorts` struct bundling ports;
`IndexChannelSource` passed as `Option<Arc<dyn …>>`.

### Exploration 3 — LiveKit dependency footprint (Explore agent)

Grep sequence: manifest scan for `livekit|livekit-api|livekit-protocol|tddy-livekit*|tddy-daemon-livekit`;
`use` scan for the same crate paths in `src/`; fully-qualified path scan; Cargo comments with
`-B3`; `livekit` in kernel `config.rs`; `.livekit` field reads; `LiveKitConfig`; spawner;
daemon-livekit module docs; graph-test search (`cargo_metadata|must_not_depend|dependency_boundary|crates_reachable_from|manifest_of`);
`feature = "livekit"`; `peer_forwarding`; per-module SDK counts.

Per-crate classification (normal deps unless noted):

| Crate | LiveKit edge | Uses it for |
|---|---|---|
| tddy-agent-launch | `tddy-daemon-livekit` | peer forward of StartSession, room/admission registries, identity strings; **also** LiveKit-free `session_git`, `os_user_resolution`, `placement` |
| tddy-cli-sessions | `tddy-livekit` | terminal bridge participant (`livekit_bridge.rs`) |
| tddy-coder | `tddy-livekit` (+ dev testkit, livekit) | session RPC participant, tokens, presence (`run.rs`, `session_participant/`) |
| tddy-daemon-auth | `livekit`, `tddy-livekit` | token minting; OAuth loopback tunnel over common room |
| tddy-daemon-kernel | `livekit`, `tddy-livekit` | **`peer_forwarding` only** (`CommonRoom`, `RpcClient`) |
| tddy-daemon-rpc | `tddy-daemon-livekit`, `livekit` (unused) | peer routing/forwarding for exec-tool, project mutations |
| tddy-daemon | `livekit`, `tddy-daemon-livekit` | composition root (`runtime.rs`), credential sync, key directory |
| tddy-discovery | feature `livekit` only | roster over tool-client LiveKit transport |
| tddy-host-service | `livekit` | names the `Room` slot for kernel `CommonRoom` |
| tddy-remote-git-repo | `tddy-livekit` | client transport (bin) |
| tddy-screen-sharing | `tddy-livekit` | token minting, config strings |
| tddy-screenshare | `livekit`, `livekit-api`, `tddy-livekit` | video track publishing (consumed only by rdp/vnc bins) |
| tddy-session-activity | `tddy-daemon-livekit` | `SessionRoomRegistry` close on delete |
| tddy-session-agents | `tddy-daemon-livekit`, `tddy-livekit`, `livekit` | clone client, broadcast, peer forwards, registries — deepest consumer |
| tddy-session-files | `tddy-daemon-livekit` | peer forward of host-document reads |
| tddy-session-lifecycle | `livekit` (unused), `tddy-livekit`, `tddy-daemon-livekit` | implements session_room ports, owns registry, re-exports 9 plugin modules (`lib.rs:86-92`) |
| tddy-session-split | `tddy-daemon-livekit`, `tddy-livekit` | split agent tokens, forwards, remote checkout room |
| tddy-session-sync | `tddy-livekit` | room client (bin + lib) |
| tddy-session-tool-client | optional feature | LiveKit tool transport |
| tddy-telegram-control | `tddy-daemon-livekit` | `SessionRoomRegistry` close on delete |
| tddy-terminal-rpc | optional feature | pty-relay LiveKit client |
| tddy-tools | feature `livekit` (default on) | turns on the three above |
| tddy-worktree-service | dev only | tests; normal closure still reaches it via kernel |
| tddy-e2e | optional feature | test harness |
| tddy-desktop (src-tauri) | none directly | embeds the daemon |

`LiveKitConfig` (`tddy-daemon-kernel/src/config.rs:1049-1110`): `enabled`, `url`, `api_key`,
`api_secret`, `public_url`, `common_room`, `common_room_set_metadata_timeout_secs`;
`common_room_enabled(Option<&Self>)`. Read in ≥18 production places across 8 crates.

`tddy-daemon-livekit` modules: `common_room_supervisor`, `first_admission_token`,
`livekit_peer_discovery` (re-exports kernel `peer_forwarding` and `daemon_identity` names),
`livekit_peer_transport`, `livekit_rooms_stream`, `livekit_service`, `os_user_resolution` (no
LiveKit), `peer_routing`, `placement` (no LiveKit), `session_admission_service`, `session_git`
(no LiveKit), `session_room`. No cycles; it depends on kernel, host-service, worktree-service,
daemon-sandbox, credential-sync, core, workflow, rpc, service.

Binaries: `tddy-coder` (55-line main; wiring in lib `run.rs`), `tddy-daemon` (204; root in lib
`runtime.rs`), `tddy-desktop` (14), `tddy-tools` (211), `tddy-remote-git-repo` (112),
`tddy-session-sync` (137), `tddy-livekit-screen-capture`, `tddy-rdp`, `tddy-vnc`, sandbox bins.

Graph tests: the two `dependency_boundary_unit.rs` copies (`crates_reachable_from`,
`workspace_dependencies` text parser); `sandbox_app_dependency_reverses.rs` (`cargo metadata
--format-version 1 --no-deps`); `kernel_surface_acceptance.rs`
(`reads_a_dependency_declared_as_a_sub_table_or_a_workspace_inherit`);
`telegram_extraction_shape.rs` (`toml`-based `manifest_of`, `every_dependency_table_of`). None is
workspace-wide; none forbids LiveKit.

### Exploration 4 — integration status surface (Explore agent)

Grep sequence: discovery-loop structs/logs; `connect` in `tddy-livekit/src`;
`ConnectionState|connection_state|LiveKitStatus|IntegrationStatus`; supervisor fns;
`run_with_reconnect`; proto `rpc .*(Health|Status|GetConfig|…)`; `livekit` in protos;
`DaemonConfigService`; `/api/config`; supervisor wiring in `runtime.rs`; `daemon_settings.rs`;
web `livekit|LiveKit`, `connectionStatus|livekitEnabled|apiSecret`; desktop `api_secret|desktop.yaml`.

- Supervisor (`tddy-daemon-livekit/src/common_room_supervisor.rs`): `trait CommonRoomSupervisor {
  fn reconfigure(&self, livekit: Option<LiveKitConfig>); }` (`:40-44`);
  `CommonRoomTarget::from_livekit` → `None` for disabled **or** incomplete (`:59-89`);
  `SupervisedCommonRoom { desired: watch::Sender<Option<CommonRoomTarget>> }` (`:140-142`);
  `connector.connect(target)` returns no `Result` (`:189-235`).
- Discovery loop (`livekit_peer_discovery.rs:840-878`): error logged then dropped; `room_slot`
  `Some` only while connected. Room events at `:1220/1230/1237/1263/1273`.
- RPC participant `run_with_reconnect_metadata` (`tddy-livekit/src/participant.rs:451-473`):
  `Err(e) => log::error!("LiveKit connect failed: {e:#}")` — no retry.
- Protos in `tddy-service/proto/`. `daemon_config.proto` `service DaemonConfigService { GetConfig;
  UpdateConfig; GetClientConfig }` — `LiveKitSettings { url, public_url, api_key, api_secret
  (write-only), common_room, api_secret_set, enabled }`. Impl
  `tddy-daemon/src/daemon_config_service.rs` (`DaemonConfigServiceImpl { config_path, config,
  common_room: Arc<dyn CommonRoomSupervisor>, authenticate }`), registered `runtime.rs:1613-1622`.
- Web: settings at `components/settings/DaemonSettingsScreen.tsx:88-151` (LiveKit block, `:89`
  natural badge spot); browser's own common-room status in `hooks/useCommonRoom.ts`. Nothing shows
  the daemon's own LiveKit status.
- Hot-apply: `UpdateConfig` calls `common_room.reconfigure` only when `(enabled, url,
  common_room)` changed (`daemon_settings.rs:124-133`) — key/secret edits are not applied and not
  reported as restart-required.

### Exploration 5 — request transport and deferred-work scan (parent, Bash)

- `packages/tddy-rpc/src/message.rs:1-80` — `RequestTransport` (host-stamped, no default),
  `RequestMetadata::over(transport)`, `.transport()`.
- `grep '\.transport()'` → `tddy-github/src/auth_service.rs:218,332` read
  `request.metadata().transport()`; `tddy-daemon-auth/src/first_login_admission.rs:98-105` matches
  on it.
- WIP changesets: `ls docs/dev/1-WIP/` — `2026-09-09-livekit-rooms-panel-every-connection.md`
  (web rooms panel; no overlap with Rust ports).
- TODO grep `livekit|session.room|ensure_session_room|plugin|extension port` → 67 hits; read in
  full: `2026-09-06-from-the-2026-09-06-optional-livekit-stack-evaluation.md` ("Is LiveKit
  configured?" decided in ≥6 places — resolve once),
  `2026-09-10-the-dependency-boundary-harness-is-duplicated-per-crate.md`,
  `2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md`,
  `2026-09-10-tddy-livekit-depends-on-tddy-service-for-two-call-sites.md`.
- Code-issue grep → 19 hits; relevant: kernel `heavy-dependency-livekit-peer-forwarding.md`
  (unclaimed), daemon-livekit `oversized-file-session-room.md`,
  `oversized-file-livekit-peer-discovery.md`, kernel `oversized-file-config.md`. Claimed-issue
  grep over the affected crates → none (the only claimed hit is in `tddy-workflow-recipes`).
- `docs/dev/changesets/2026-09-06-lazy-session-room.md` read in full (the #451 rationale quoted in
  Combined Conclusions §2).

## Exploration 2 — this node

1. `grep -rnE 'TokenGenerator|DEFAULT_LIVEKIT_JWT_TTL_SECS|LiveKitTokenProvider|SessionTokenMinter' packages/*/src` excluding plugin crates — sites in cli-sessions `livekit_bridge.rs:5,162`; coder `run.rs:69-72,1980-1987,2229-2234,3550-3557,3680-3687`; daemon-auth `token_provider.rs:1-6`, `auth.rs:12,330-338,523`; livekit-screen-capture `config.rs:101`; screen-sharing `screen_sharing_service.rs:503-508`; service `token_service.rs:17`; session-agents `session_agent_clone.rs:28,529`; session-split `split_session.rs:185`, `agent_credentials.rs:107`; session-sync `attach.rs:29,269-274`; terminal-rpc `pty_relay.rs:306,318`.
2. `grep -n 'pub fn new|generate|pub struct TokenGenerator' packages/tddy-livekit/src/token.rs` — `:18,27,44,50`.
3. Read `tddy-service/src/token_service.rs:1-60` — `TokenProvider` trait, `TokenServiceImpl::{unauthenticated, authenticated}`.
4. Read `tddy-screen-sharing/src/screen_sharing_service.rs:480-520`; `tddy-daemon-auth/src/auth.rs:318-345, 510-535`; `token_provider.rs` (whole).
5. `grep -n build_token_service_entry` — `auth.rs:319,1100,1231,1254`, `tddy-daemon/src/runtime.rs:758`. `grep -n 'MINTED_ROOM_TOKEN_TTL|^pub struct|^impl.*for' auth.rs` — `:411 CommonRoomSwitch`, `:437`, `:455 LiveKitTokenServiceImpl`, `:470`.
6. Read TODO `2026-08-13-tokengenerator-generate-for-performs-no-authorization.md` (first 25 lines).
