# Initial Discovery: e2e-leg — shared LiveKit CI

**Changeset**: [2026-10-04-e2e-leg-shared-livekit-ci.md](./2026-10-04-e2e-leg-shared-livekit-ci.md)
**Date**: 2026-10-04
**Passes**: 2 (whole-work discovery for the `#e2e-leg` stack, copied in full)

## Combined Conclusions

The todo note is mostly right, but both passes found places where it is stale or too optimistic.
These corrections drive the plan:

1. **Isolation is a smaller migration than the note says.** ~10 of the 34 LiveKit test files already
   build unique room names (`a_room_for`, `a_room_named` — nanos; inline `uuid`; `COMMON_ROOM_PREFIX`).
   The files that share a fixed room across several tests: `session_agent_remote_acceptance` (22 tests,
   1 room), `session_attach_cross_host_acceptance` (8), `multi_host_acceptance` (6),
   `session_sync_livekit_acceptance` (6), `livekit_peer_daemons_acceptance` (3), `relay_e2e_acceptance`
   (3), `staging_forwarding_acceptance` (2), `token_service_livekit` (2), `token_generation_livekit` (2),
   plus `rpc_scenarios`' 10 per-block rooms (one test, unique within the binary). The only literal
   collision **across binaries** is `"acceptance-common-room"` (`livekit_peer_daemons_acceptance` ×
   `multi_host_acceptance`).
2. **The note's `"tddy-lobby"` collision is not real for the LiveKit server.** `token_service_acceptance`,
   `cross_crate_session_token_acceptance`, `session_tool_livekit_dispatch` never start `LiveKitTestkit`;
   `tddy-lobby` is a config value / token claim there.
3. **Of the 5 "server-global" tests the note names, only 2 touch a real server.**
   `stream_livekit_rooms_rpc` (scripted roster), `room_roster_deadline` (silent TCP listener) and
   `local_token_uds` (stub roster) never use the testkit. `room_roster_livekit` and
   `session_room_acceptance::room_on_the_server` read the whole roster but look up their own unique room.
4. **The real shared-server hazard is the roster read.** `LiveKitRoomRoster::read_roster`
   (`tddy-livekit/src/room_roster.rs`) does `list_rooms` then one sequential `list_participants` per room
   inside a 5s `ROSTER_READ_TIMEOUT`, and a failing `list_participants` aborts the whole read. With many
   rooms churning on a shared server this can fail spuriously. **Unverified**: whether LiveKit errors or
   returns empty for a room that closed between the two calls.
5. **`#[serial]` is moot under nextest** (one process per test). The `docker` test-group
   (`max-threads = 1`) is the only serialisation; many multi-test files are safe today only because of it.
   Lifting the group without unique rooms exposes every collision above.
6. **`run-livekit-testkit-server` is not a drop-in model for the CI start step.** It publishes fixed
   container ports 7880/7881/7882 on random host ports and sets neither `--config-body` nor `UDP_PORT`,
   whereas `LiveKitTestkit::start()` documents that host port MUST equal container port because LiveKit
   embeds container ports in ICE candidates. **Unverified** whether media works through the script's
   shape. The CI step should copy `start()`'s host=container mapping.
7. **nextest 0.9.132 (nix store, from embedded strings — not docs).** Setup scripts exist
   (`[scripts.setup.*]`, `NEXTEST_ENV`) behind `[experimental] setup-scripts = true`; there is **no
   teardown hook**; `slow-timeout` supports `period`, `terminate-after`, `grace-period`. So the CI
   workflow steps own the server lifecycle, as the note proposed.
8. **The testkit already** has `remove_participant` (the 2026-10-04 local fix is on this branch and the
   test uses it), its env-URL timeout error already names the URL, and it has no `unique_room`, no stop
   and no container accessor. Image tag is floating `master`, in both testkit and script.
   `remove_participant` only handles `ws://`.
9. **The `peer_forward_timeout_seconds` setting is a bigger change than "thread it to the call sites".**
   ~17 unary + 7 stream production call sites go through functions that take only
   `room_slot: &Arc<RwLock<Option<Arc<Room>>>>`. Only `PeerRouting`, `HostServiceImpl` and the lifecycle
   `ConnectionService` family hold a `DaemonConfig`; `tddy-session-agents` forwarders and
   `tddy-daemon-rpc` exec-tool ports hold only a slot. Options: (a) add a deadline argument everywhere
   (~25 edits); (b) carry the deadline with the slot; (c) a process-wide value — smallest, but several
   in-process daemons share one process in the e2e tests, so a global is wrong. **Needs a decision in
   that node's PRD.** `forward_server_stream_to_peer` uses the constant directly (open timeout + status),
   and `svc_spawn_split_agent.rs:415` (`split_forward_deadline`) and
   `remote_managed_worktree_acceptance.rs:633` also read `PEER_FORWARD_TIMEOUT`. The idle timeout stays
   fixed (guard test + `connection_service.rs` const assert).
10. **Naming:** existing durations on `DaemonConfig` are `_secs` (`spawn_worker_request_timeout_secs`,
    `common_room_set_metadata_timeout_secs`); `_seconds` only on `github.pending_login_ttl_seconds` and
    `open_vault_idle_ttl_seconds`. The note's `peer_forward_timeout_seconds` is a deliberate choice, not a
    convention.
11. **`rpc_scenarios`** is 1253 lines, one `#[serial]` test, 10 blocks, one shared testkit, each block with
    its own `room_name`; its inner timeouts are 3s/5s/10s/10s plus one `sleep(2s)` and
    `PARTICIPANT_TIMEOUT` 10s — the note's "10–20s" is overstated. The 30.8s is the sum of the blocks,
    so the split is mechanical and its payoff only arrives once the docker group is lifted.
12. **CI facts.** `rust-test` matrix `Rust tests` / `Rust e2e tests`, `timeout-minutes: 150`, no step
    timeouts, no Docker step, no `LIVEKIT_TESTKIT_WS_URL`, both legs `nextest run --workspace --profile ci
    -E <filter>`; e2e filter comes from `.config/rust-e2e.filterset`; rust-cache `shared-key: test`, saved
    only by the unit leg on master. `./dev` execs `nix develop -c`, so `$GITHUB_ENV` reaches nextest.
    `Rust e2e tests` is not a required branch-protection context.
13. **The docker-group membership list is by binary name and package** and its set difference with the
    filterset was not re-derived — treat exact membership as unverified; node 5 (parallel-livekit) must
    derive it.
14. **Step 2b (backlog / code issues).** No `**Claimed by:**` code issue is in the path. Relevant records:
    `tddy-daemon-kernel/docs/code-issues/oversized-file-config.md` (adding a `DaemonConfig` field must
    update its table — *during*), `heavy-dependency-livekit-peer-forwarding.md` (unclaimed, `peer_forwarding`
    is the module the deadline node edits — *during*), `tddy-daemon-livekit/docs/code-issues/
    oversized-file-livekit-peer-discovery.md` (the deadline node touches ~10 call sites in it — *during*).
    Todo entries: the source note itself (claimed by the stack, split across nodes).

## Exploration 1: LiveKit test rooms, testkit, nextest, CI — 2026-10-04

**Agent**: Explore subagent
**Scope**: every `LiveKitTestkit` user, `tddy-livekit-testkit`, `.config/nextest.toml`,
`.config/rust-e2e.filterset`, `.github/workflows/ci.yml`, `docs/dev/guides/ci.md`, nextest version.

### Sequence

1. Read the todo note. 2. `grep -rln LiveKitTestkit` over `*.rs`; read nextest.toml and rust-e2e.filterset.
3. Read testkit `livekit_testkit.rs`, `lib.rs`, `Cargo.toml`, its integration test, `run-livekit-testkit-server`.
4. Grep every LiveKit test file for `const …ROOM…`, `generate_token(`, `Uuid::new_v4`, `room_name =`,
   `COMMON_ROOM_PREFIX`. 5. Grep `"tddy-lobby"`, `list_rooms|list_participants|ListRooms|ListParticipants`.
6. Read `room_roster_livekit.rs`, `room_roster.rs`, `livekit_rooms_stream.rs`, `session_room_participants.rs`,
   `session_room_acceptance.rs`. 7. Read `ci.yml` (493 lines), `docs/dev/guides/ci.md`, `./dev`.
8. `strings` on the nextest 0.9.132 store binary for `setup-scripts`, `teardown`, `slow-timeout`, `NEXTEST_ENV`.

### Inspected files (excerpts)

`packages/tddy-livekit-testkit/src/livekit_testkit.rs`:

```rust
const LIVEKIT_IMAGE: &str = "livekit/livekit-server"; const LIVEKIT_TAG: &str = "master";
const DEV_API_KEY = "devkey"; DEV_API_SECRET = "secret";
const API_READY_TIMEOUT = 15s; API_READY_INTERVAL = 200ms;
pub const LIVEKIT_TESTKIT_WS_URL_ENV: &str = "LIVEKIT_TESTKIT_WS_URL";
pub struct LiveKitTestkit { _container: Option<ContainerAsync<GenericImage>>, ws_url: String }
```

`start()`: env var set → `parse_ws_url`, `wait_for_api_url_async("http://host:port")`, `_container: None`.
Otherwise three ports from `free_tcp_port()` / `free_udp_port()` (bind `:0` and release), image run with
`--dev --bind 0.0.0.0 --node-ip 127.0.0.1 --config-body "port: N\nrtc:\n  tcp_port: M\n"`, `UDP_PORT`
env, host:N→container:N maps, `HttpWaitStrategy("/")`, then API poll. Public API: `start`, `get_ws_url`,
`remove_participant(room, identity)`, `generate_token(room, identity)` (TTL 3600s). Only `LiveKitTestkit`
re-exported from `lib.rs`; the env-var const is not.

`./run-livekit-testkit-server`: container `tddy-livekit-testkit`, `-p 0:7880 -p 0:7881 -p 0:7882/udp`,
`livekit/livekit-server:master --dev --bind 0.0.0.0 --node-ip 127.0.0.1`; prints
`export LIVEKIT_TESTKIT_WS_URL=…`; no `--stop`, no pin.

`room_roster.rs` `read_roster`:

```rust
let rooms = self.client.list_rooms(vec![]).await.map_err(...)?;
for room in rooms { let joined = self.client.list_participants(&room.name).await
   .map_err(|err| format!("livekit ListParticipants({}) failed: {err}", room.name))?; ... }
```

### Grep / glob — test files by room strategy

| File (package) | Rooms | Tests | Note |
|---|---|---|---|
| acp_session_livekit (tddy-livekit) | `acp-session-scenarios` | 1 | |
| broadcast_and_room_metadata | `a_room_for(purpose)` nanos | 7 | already unique |
| participant_metadata_acceptance | `acceptance-owned-project-count` | 3 | |
| room_roster_livekit | `a_room_named("roster-…")` nanos | 2 | reads whole roster |
| rpc_client_factory | `factory-basic/-concurrent/-singleton` | 3 | distinct |
| rpc_scenarios | 10 block rooms; `bidi-token-refresh` | 2 | one 10-block test |
| rpc_three_participant_forward | 3 distinct | 3 | |
| common_room_duplicate_identity_repro (daemon-livekit) | `repro-dup-common-room` | 1 | |
| common_room_set_metadata_handshake_repro | `repro-metadata-handshake-room` | 1 | |
| forward_to_peer_shared_registry | `forward-shared-registry` | 1 | |
| forwarded_rpc_is_stamped_by_the_receiver | `COMMON_ROOM_PREFIX`-uuid | 1 | pattern to copy |
| livekit_peer_daemons_acceptance | `acceptance-common-room` | 3 | **collides in-binary + cross-binary** |
| session_room_livekit_acceptance | derived from session id | 6 | |
| session_room_exec_tool_acceptance (daemon-rpc) | `session-room-lobby` | 1 | |
| common_room_key_trust_acceptance (daemon) | `key-trust-lobby-<uuid>` | 3 | unique |
| first_login_enrolment_acceptance | `first-login-<uuid>` | 9 | unique |
| multi_host_acceptance | `acceptance-common-room` | 6 | **collides** |
| relay_e2e_acceptance | `relay-e2e-common-room` | 3 | in-binary collision |
| remote_managed_worktree_cross_host_acceptance | `split-placement-room-<uuid>` OnceLock | 10 | per-process |
| runtime_signing_identity_acceptance | `runtime-signing-lobby-<uuid>` | 1 | unique |
| session_agent_remote_acceptance | `agent-roster-common-room` | 22 | **22 tests, 1 room** |
| session_attach_cross_host_acceptance | `attach-cross-host-room` | 8 | in-binary collision |
| session_room_cross_host_acceptance | `session-room-split-lobby-<uuid>` OnceLock | 4 | per-process |
| split_session_resume_acceptance | `split-resume-room-<uuid>` OnceLock | 8 | per-process |
| staging_forwarding_acceptance | `attach-start-forwarding-room` | 2 | in-binary collision |
| livekit_terminal_rpc (tddy-e2e) | 4 distinct | 5 | |
| terminal_service_livekit | 2 distinct | 3 | |
| token_generation_livekit | `token-generation-test` | 2 | in-binary collision |
| token_service_livekit | `token-service-test` | 2 | in-binary collision |
| coder_publishes_session_metadata… (tddy-coder) | 1 | 1 | |
| coder_serves_connection_service… | 2 distinct | 2 | |
| session_room_acceptance (session-lifecycle) | `COMMON_ROOM_PREFIX`-uuid via `a_lobby_of_its_own()` | 21 | unique |
| session_sync_livekit_acceptance | `session-sync-e2e-lobby` | 6 | in-binary collision |
| remote_git_livekit_acceptance (worktree-service) | `tddy-remote-git-{suffix}` | 13 | caller supplies suffix |
| livekit_testkit_integration (testkit) | `test-room` | 2 | token only, never connects |

Identities: per-file constants (`server`/`client` generic; `daemon-{instance_id}` for RPC participants,
bare `instance_id` for the announcing one); identity is room-scoped so unique rooms make them safe.

**nextest.toml**: `[profile.ci]` fail-fast false, `retries = {exponential, count 2, delay 5s, jitter}`,
JUnit `junit.xml`; `default-filter` excludes six sandbox/cgroup tests; `[test-groups]` `docker`
(max-threads 1) and `rust-analyzer` (max-threads 1); `profile.default` override puts 10 of the 31
restructuring binaries in `rust-analyzer`; `profile.ci` override puts ~40 LiveKit binaries (by package
`tddy-livekit`, `tddy-livekit-testkit` and named binaries) in `docker`. No `slow-timeout`,
`leak-timeout`, `global-timeout`, `threads-required` anywhere.

**rust-e2e.filterset**: the LiveKit binaries above plus `package(tddy-supervisor) and kind(test)`,
`dual_transport_acceptance`, `ping_answers_only_a_live_listener`, `index_daemon_client_acceptance`,
`acceptance_daemon`, `stdio_remote_control_acceptance`, `package(tddy-code-restructuring)` and 31 named
restructuring binaries.

**ci.yml `rust-test`**: matrix `Rust tests` (`junit-rust`) / `Rust e2e tests` (`junit-rust-e2e`);
`needs: rust-build`; `timeout-minutes: 150`; steps: checkout@v7, reclaim disk, install-nix-action@v31,
cache-nix-action@v7 (`save: false`), rust-cache@v2 (`shared-key: test`,
`save-if: master && !matrix.e2e`), download `rust-fixture-bins`, install fixture bins into
`target/debug`, `./dev cargo nextest run --workspace --profile ci --locked -E "$filter"`
(`continue-on-error`), action-junit-report@v6, upload JUnit artifact. Other jobs: `rust-lint`,
`rust-build` (nix-cache writer, stages fixtures), `rust-build-arm64`, `generated-code`, `web`.

**docs/dev/guides/ci.md**: six checks table (cold times blank for the Rust legs); documents the
`docker` group and retries as the TOCTOU mitigation (cites `livekit_testkit.rs:26`); states
`Rust e2e tests` is not a required context; "three separate keys" cache statement is stale.

**nextest**: `flake.nix:73` `pkgs.cargo-nextest`, `nixos-unstable` pinned in `flake.lock`; store path
`…-cargo-nextest-0.9.132` present locally. Strings: `scripts.setup`, `setup-script`, `NEXTEST_ENV`,
`capture-stdout`, `experimental … setup-scripts`, `slow-timeout`/`terminate-after`/`grace-period`/
`on-timeout`, `global-timeout`, `leak-timeout`; zero `teardown`.

### Findings

See Combined Conclusions 1–8, 12, 13.

## Exploration 2: deadline setting, forward call sites, rpc_scenarios — 2026-10-04

**Agent**: Explore subagent
**Scope**: `peer_forwarding`, daemon config, `rpc_scenarios`, the metadata repro, docs for a new setting.

### Sequence

1. Read the todo §2. 2. Repo-wide grep for `PEER_FORWARD_TIMEOUT`, `PEER_FORWARD_STREAM_IDLE_TIMEOUT`,
   `PASS_LONG_ENOUGH_TO_BE_SERVICE`, the three forward fns, `peer_forward_timeout`, the deadline test,
   `fn rpc_scenarios`. 3. Read `peer_forwarding.rs:60-300`, `livekit_peer_discovery.rs` (re-exports, typed
   wrappers), `session_attach_cross_host_acceptance.rs`, `session_agent_roster_acceptance.rs:580-625`,
   `peer_routing.rs`, `tddy-host-service/src/service.rs`, every remaining call site.
4. Grep `spawn_worker_request_timeout`, `pending_login_ttl_seconds`, `common_room_set_metadata_timeout_secs`.
5. Read `tddy-daemon-kernel/src/config.rs` (DaemonConfig 312-500, LiveKitConfig 1010-1060, accessors,
   tests), `daemon.yaml.production`, `dev.daemon.yaml`, `docs/ft/daemon/daemon-settings.md`.
6. Read `rpc_scenarios.rs`, `common_room_set_metadata_handshake_repro.rs`, `.config/nextest.toml:110-160`.

### Inspected files (excerpts)

`packages/tddy-daemon-kernel/src/peer_forwarding.rs`: `PEER_FORWARD_TIMEOUT = 30s` (:93),
`PEER_FORWARD_STREAM_IDLE_TIMEOUT = 30s` (:106, bounded below by `PASS_LONG_ENOUGH_TO_BE_SERVICE`, pinned
by a test since `tddy-tools` is only a dev-dependency). `forward_to_peer` (:159) is a one-line wrapper
over `forward_to_peer_within(…, deadline)` (:184) = `tokio::time::timeout(deadline, client.call_unary)`.
`forward_server_stream_to_peer` (:206) uses the constants directly: :220 open timeout, :224 status,
:246 per-frame idle, :256 idle status. Status text: `Status::deadline_exceeded("forwarding {service}/
{method} to daemon {peer} timed out after {}s: the peer is in the common room but its RPC participant
did not answer")`.

Guard test `session_agent_roster_acceptance.rs:~590`:
`tears_a_forwarded_stream_down_no_faster_than_a_pass_needs_to_last_to_count_as_service` asserts
`PEER_FORWARD_STREAM_IDLE_TIMEOUT >= PASS_LONG_ENOUGH_TO_BE_SERVICE`; unaffected by a unary-deadline
setting. Related: `connection_service.rs:60-78` const assert `ROSTER_KEEPALIVE_INTERVAL (8s) * 2 <
PEER_FORWARD_STREAM_IDLE_TIMEOUT`; `session_tool_livekit_dispatch.rs:284-291` mirrors 30s.

Deadline test: `session_attach_cross_host_acceptance.rs:441`
`a_forwarded_rpc_to_a_peer_that_stopped_answering_fails_within_its_deadline`: `#[serial]`, `two_daemons()`,
`peer_rpc_run.abort()`, `timeout(60s, upload_staged_attachment_chunk(…))`, asserts `Code::DeadlineExceeded`.
Comment to update at :447-450 ("60s … comfortably longer than the deadline"); other 30s outer waits
reference the constant at :412, :602, :779. `two_daemons()` (~:258) builds the daemons directly.

Production forward call sites — unary `forward_to_peer`: `peer_routing.rs:170`;
`livekit_peer_discovery.rs:754 (ListProjects, inside 3s fan-out), 1438, 1488, 1510, 1536, 1557, 1578,
1598`; `tddy-host-service/src/service.rs:327`; `svc_activity_ports.rs:249`; `svc_turn_end_reporter.rs:104`;
`session_agent_clone.rs:787`; `conversation_open_forward.rs:24`; `conversation_cancel_forward.rs:16`;
`exec_tool/ports.rs:223, 303`. Streams `forward_server_stream_to_peer`: `peer_routing.rs:210`;
`livekit_peer_discovery.rs:1413, 1464, 1624`; `service.rs:360`; `svc_session_agent_port_adapters.rs:259,
291`. `forward_to_peer_within`: only `livekit_peer_discovery.rs:1387`
(`forward_start_session_via_livekit_within`; the non-`within` twin at :1362 passes the constant).
Typed `forward_*_via_livekit` wrappers (~20 callers) take only `room_slot`.

Other readers of the constant: `svc_spawn_split_agent.rs:411-418`
`split_forward_deadline() = spawn_worker_request_timeout() + PEER_FORWARD_TIMEOUT`;
`remote_managed_worktree_acceptance.rs:24,633` asserts `600s + PEER_FORWARD_TIMEOUT`;
`docs/ft/daemon/remote-managed-worktree.md:422,431,459`; generated `session_files_pb.ts` and
`session_files.proto:40` mention it in a comment.

Config: `DaemonConfig` (`config.rs:312`) precedent `spawn_worker_request_timeout_secs: u64`
(`#[serde(default = "default_…")]`, default 300, accessor with `.max(1)`, tests in
`mod spawn_timeout_tests`, commented example at `daemon.yaml.production:116`). `LiveKitConfig` has
`deny_unknown_fields`; sibling `common_room_set_metadata_timeout_secs` (default 60). Holders of a
`DaemonConfig`: `PeerRouting` (`peer_routing.rs:20`, built at `svc_host_builders.rs:61`),
`HostServiceImpl`, lifecycle `ConnectionService` family. Slot-only: `tddy-session-agents` forwarders and
`tddy-daemon-rpc` exec-tool ports. Docs: key list is in config comments and `daemon.yaml.production`;
feature docs `docs/ft/daemon/remote-managed-worktree.md`, `livekit-peer-discovery.md`;
kernel docs `packages/tddy-daemon-kernel/docs/daemon-kernel.md:174`; changeset template
`docs/dev/changesets/2026-09-24-keyring-store.md`; `oversized-file-config.md` table must be updated.

`packages/tddy-livekit/tests/rpc_scenarios.rs` (1253 lines): `#[tokio::test] #[serial] async fn
rpc_scenarios()` at :392–~1108, one `LiveKitTestkit::start()` (:411). Fixtures `CountingEchoService`,
`CountingHarness`, `TestHarness`, `LoopbackTunnelHarness`, `ThreeParticipantHarness`,
`wait_for_participant` (`PARTICIPANT_TIMEOUT` 10s), each `start(livekit, room_name)` + `teardown()`.
Blocks → room: unary 417-559 `unary-scenarios`; server stream 565-660 `stream-scenarios`; client stream
666-693 `client-stream-scenarios`; bidi 699-738 `bidi-stream-scenarios`; realtime 745-812
`realtime-stream-scenarios` (10s/3s); response isolation 820-857 `response-isolation`; stateful bidi
863-926 `stateful-bidi-scenarios` (5s); ordering 936-985 `bidi-input-order`; loopback 991-1044
`loopback-tunnel-scenarios` (10s); duplicate identity 1054-1102 `duplicate-identity` (own `Room`s,
`sleep(2s)` :1093). Second test `bidi_stream_survives_delay_without_app_reconnect` (:1113, `bidi-token-refresh`).
Logger `collector.install()` is per process, tolerated twice with `let _ =`.

`common_room_set_metadata_handshake_repro.rs` (121 lines): the test polls the room slot (60s ceiling,
400ms), then `sleep(12s)` and asserts the slot is still `Some` — the 12s is the property; in the
`docker` group (:125, :152) and the filterset.

### Findings

See Combined Conclusions 9–11.
