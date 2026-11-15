# Initial Discovery: One remote-shell port, SSH as its first transport

**Changeset**: [2026-11-15-remote-shell-shell-port.md](./2026-11-15-remote-shell-shell-port.md)
**Stack**: `#remote-shell 1/7` — `feature/remote-shell/shell-port`
**Date**: 2026-11-15
**Passes**: 4

> Explorations 1–4 are the whole-stack discovery, copied in full: every PR of this stack was planned from
> the same passes. Node-specific follow-up is appended as later Exploration sections.

## Combined Conclusions

Three passes (LiveKit remote path, accounts/hosts, split/SSH/PTY). Actionable reading of State A:

1. **"Remote" today always means a peer `tddy-daemon` reached over LiveKit.** Hosts come only from
   the common room (`LiveKitEligibleDaemonSource`; web `livekit` host-directory source). A remote
   start is `classify_peer_route` → `PeerForwarder` (`LiveKitPeerForwarder` is the only production
   impl) → the peer creates the session on its own disk. The browser then talks to the peer
   directly; the serving daemon is not in the terminal byte path, and `PeerForwarder` has no bidi
   stream.
2. **LiveKit is already behind ports on the daemon side** (`PeerForwarder`, `TerminalExposure`,
   `SessionReach`, `EligibleDaemonSource`, … each with a null object; satellite ratchet test) and
   behind `ConnectionProvider`/`HostConnection` on the web. What still assumes LiveKit: wire names
   (`livekit_room`, `livekit_*` on Connect/Start replies), the `daemon-{id}` identity scheme,
   error strings, `tddy-spawn` argv, host discovery.
3. **The premise "SSH exists for split session" is not what the code does.** A split session is a
   two-daemon placement over LiveKit (agent on A, worktree on daemon B). SSH is a separate axis:
   `StartSessionRequest.ssh_config_host`, an OpenSSH alias listed by `ListSshConfigHosts`. With it
   set, the code-managing daemon runs worktree setup, exec-catalog tools (`tddy_tool_engine::Shell`
   → `RemoteShell`), slot files, the session pointer and git packs as one-shot `ssh -o BatchMode=yes
   alias 'cmd'`. **The agent itself always runs in a local PTY**; there is no `ssh -tt`, no remote
   PTY. `record_remote_owner`/`REMOTE_AGENT_PID_FILE` are dead code awaiting an ssh agent launch
   (todo 2026-11-07-ssh-agent-launch-remote-pointer-owner).
4. **Four separate ssh spawn sites** with diverging options: tool-engine `RemoteShell`, `tddy-git`
   `ssh_exec`/`ssh_worktree` (hard-coded `/home/dev/<repo>`), agent-launch `run_over_ssh`
   (`PointerHost::{Local,Ssh}`), worktree-service `PackExecution::Remote`. A transport-agnostic
   remote shell has to unify: one-shot exec (stdin, deadline), streamed exec, file read/write —
   and add an **interactive PTY**, which does not exist.
5. **PTY seam.** Every agent launch ends in `CliSessionManager::spawn_rendered_main` →
   `tddy_terminal_rpc::PtyRuntime` → `tddy_pty` (`portable_pty`, `MasterPty` typed through
   `PtyReady`/`PtyHandle`/`PtyRegistry`). `TerminalSession`/`TerminalSessionStore` are already
   transport-agnostic. A local PTY whose child is `ssh -tt alias …` / `coder ssh ws …` keeps every
   consumer unchanged and makes the serving daemon the mediator.
6. **Hooks.** The hooks file is rendered locally; the hook command is
   `tddy-tools session-hook --daemon <url> --daemon-socket <sock>`. Blocking hooks
   (PermissionRequest, AskUserQuestion) wait up to `answer_wait_secs` for a reply. A bare remote has
   no `tddy-tools`, so an agent running there needs a carrier built from `sh` only — see the user
   constraint below.
7. **Accounts.** Vault records `CredentialRecord{provider(open string), account, label, secret,
   metadata}` per signed-in GitHub subject. A `coder` record (url + token) fits with no schema
   change; `ListAccounts`/rename/remove/sync work for any provider string. Intake does not fit:
   `BeginLinkAccount` is GitHub device-code only, `ConnectProviderAccount` is bound to the
   model-provider preset/route table. Precedents: `tddy-screen-sharing` owning its own provider and
   codec; "Local gh" (the host's own CLI login, probed via `GetHostTooling`).
8. **Target listing.** `ListSshConfigHosts` per host feeds `ssh_config_host`. A Coder workspace
   listing is its sibling. There is no transport-tagged target model: only the `ssh_config_host`
   string in proto, `.session.yaml` and the web form.
9. **Backlog in the path** (verdicts assigned in Step 2b): ssh-agent-launch-remote-pointer-owner,
   ssh-agent-bounds-stop-at-the-conversation, warp-provider-account (CLI-tool account analogue),
   keyring add-account-ungated, web-provider-preset mirror, accounts service.rs over budget,
   peer-testkit / LiveKit-compat todos (2026-11-05, 2026-10-24), daemon links LiveKit
   unconditionally (2026-10-21), spawner read scope same-host only.

Open questions for the interview: where the agent runs for an SSH/Coder session; hook carrier and
whether blocking hooks must work; Coder credential source (vault vs host CLI login); how far the
LiveKit wire names are generalised.

## User constraints recorded during planning

- **2026-11-15 — hooks without remote installation.** The daemon must receive the coding agent's
  lifecycle hooks from an SSH or Coder remote **without any extra software installed there**
  (no `tddy-tools`, no daemon). The user flags this as the likely hard part. Candidate carriers to
  evaluate against the existing hook path: (a) hook command is a plain POSIX shell one-liner that
  appends the hook JSON to a file/FIFO the daemon tails over a second channel of the same
  connection; (b) a reverse-forwarded unix socket/port (`ssh -R`, `coder ssh --remote-forward`)
  that the hook writes to with shell built-ins; (c) an in-band marker on the PTY stream. Each must
  be checked for: what the agent's hooks config can invoke, blocking hooks that need a reply
  (permission prompts), and what a bare remote is guaranteed to have (`sh`, not `curl`/`nc`).


- **2026-11-15 — interview round 1 (user answers).** (1) The agent runs **on the remote**; the
  remote needs the agent CLI and nothing from tddy. (2) Hooks are **inline `sh` only, notify-only**:
  a one-liner in the hooks file appends JSON to a file the daemon tails; no script file is pushed and
  blocking hooks (permission prompts, questions) are not answered from the web for these sessions.
  (3) Coder credentials are a **vault account** (deployment URL + session token) on the Accounts
  screen. (4) LiveKit scope: **unified host directory** — SSH hosts and Coder workspaces appear in
  the host selector and Hosts screen as hosts derived from the active daemon; LiveKit wire names
  are not renamed.

- **2026-11-15 — hooks for every agent.** The remote hook carrier must work for **any agent whose
  spec supports hooks**, not Claude Code alone. Exploration 4 shows the seam: every hooks adapter
  renders the same shell-string command from `HookCommandParams`, so the carrier is a property of
  the command, not of the agent.

## Exploration 1: remote sessions over LiveKit — 2026-11-15

**Agent**: Explore subagent
**Scope**: web → RPC → daemon → peer daemon over LiveKit → terminal stream; transport seams; test transports

### Discovery dump: remote sessions over LiveKit (plan-remote-shell-transports)

Repo root: `/Users/mantasi/Code/tddy-coder/.worktrees/plan-remote-shell-transports` (HEAD `cb64e89a3`). The only uncommitted file is `docs/dev/1-WIP/2026-11-15-remote-shell-transports-whole-work-initial-discovery.md`, which is still a skeleton (14 lines, "Combined Conclusions _(written after the first pass)_").

**Answer in brief:**
- LiveKit is already behind ports on the daemon side. `PeerForwarder`, `TerminalExposure`, `SessionReach`, `RoomConnector`, `EligibleDaemonSource` and others each have a null object, and a ratchet test keeps every library crate free of LiveKit dependencies.
- The web side has a `ConnectionProvider` / `HostConnection` / `SessionConnection` model. LiveKit and desktop IPC are its two providers.
- The places that still assume LiveKit are:
- wire field names: `ConnectSessionResponse.livekit_*`, `SessionMetadata.livekit_room`
- the `daemon-{id}` / `daemon-{id}-{session}` identity scheme
- error strings that tell the user to configure `livekit.common_room`
- `tddy-spawn`'s LiveKit credential helpers
- the `tddy-daemon` binary, which links LiveKit unconditionally
- the host directory, which only learns about remote hosts from the common room
- No shared "peer testkit" crate exists. About 12 test files each write their own in-memory `PeerForwarder`. Two todo entries ask for one shared testkit plus a LiveKit compatibility package.

---

#### 1. Sequence (chronological)

1. `ls packages/`, `ls docs docs/ft docs/dev docs/dev/todo`, `git log -5`: map the packages and docs.
2. `grep -rl livekit --include=Cargo.toml packages`: which crates name LiveKit. Also grepped for `peer-testkit|satellite` in Cargo.toml: is a testkit or satellite ratchet present?
3. Read the head of `packages/tddy-daemon-extensions/Cargo.toml` and listed the src/tests of `tddy-daemon-extensions`, `tddy-daemon-livekit`, `tddy-livekit`, `tddy-livekit-testkit`, `tddy-daemon-kernel` and `tddy-room-client`: where the ports and the plugin live.
4. Listed `docs/ft/daemon`, `docs/ft/web`, `docs/dev/1-WIP` and the package docs dirs (`tddy-daemon`, `tddy-livekit`, `tddy-daemon-livekit`, `tddy-daemon-extensions`): find feature docs.
5. `head` of the WIP whole-work discovery doc and `git status`: check for existing work.
6. Read `packages/tddy-daemon-extensions/docs/extension-ports.md` in full: the authoritative list of ports.
7. Grepped the 10 non-plugin Cargo.tomls for `livekit`: are these real dependencies or only dev-dependencies and comments?
8. Grepped the lists in `tests/livekit_is_a_satellite.rs`: the ratchet's membership.
9. Read `tddy-daemon-extensions/src/peer_forwarder.rs` in full: the peer transport port.
10. Outlined `tddy-daemon-kernel/src/peer_forwarding.rs` and `peer_calls.rs` (pub items), then read `peer_forwarding.rs` lines 1-260 and `peer_calls.rs` lines 1-150: the `CommonRoom` forwarding policy and the typed forwards.
11. Read `tddy-daemon-livekit/src/livekit_peer_forwarder.rs`, `peer_routing.rs`, and `livekit_peer_transport.rs` lines 1-80: the LiveKit adapter.
12. Read `tddy-host-service/src/peer_routing.rs` lines 1-200 and listed `tddy-host-service` src/docs: transport-neutral routing.
13. Outlined `livekit_peer_discovery.rs` (pub items), read lines 1-97 and 820-900: discovery, advertisement and connect strings.
14. Read `tddy-host-service/src/multi_host.rs` lines 1-90: the `EligibleDaemonSource` trait.
15. `ls packages/tddy-service/proto`, grepped `daemon_instance_id|host_id` in `session.proto`, outlined the services/messages of `session.proto`, `terminal.proto`, `livekit.proto`, `session_room.proto` and `host.proto`, and read the key messages.
16. Grepped `forward_start_session_relaying|forward_stream_start_session|classify_daemon_route|PeerRoute::Forward` across packages: the StartSession routing call sites.
17. Grepped `livekit_room|livekit_url|livekit_server_identity` in session-lifecycle, agent-launch and cli-sessions: where LiveKit fields enter the handlers.
18. Read `tddy-agent-launch/src/svc_start_session_core.rs` 90-150, `start_request_checks.rs` 310-360, and `session_coordinate_handlers.rs` 180-235 and 305-345: the StartSession/StreamStartSession/ConnectSession handlers.
19. Read `docs/ft/web/daemon-selector-livekit-rpc.md` in full: how the web picks a host.
20. `ls packages/tddy-web/docs` and read `host-connections.md` in full.
21. Read `session-connections.md` and `host-directory.md` in full.
22. Read `tddy-daemon-livekit/src/terminal_exposure.rs` and `tddy-daemon-extensions/src/terminal_exposure.rs`: the PTY exposure port.
23. Read `tddy-session-lifecycle/src/connection_service/terminal_bridge_impl.rs` and grepped `ensure_exposed_terminal`.
24. Grepped `tddy-spawn/src/spawner.rs` for its LiveKit helpers and read 230-285 and 870-900. Located `SessionMetadata` (`tddy-changeset/src/session_metadata.rs`), listed its fields, and read 18-30 and 88-125.
25. Grepped `RemoteShell|LocalShell` (prompted by the `ssh_config_host` doc) and read `tddy-tool-engine/src/shell.rs` 1-200.
26. Listed `tddy-session-create-web/src` and grepped `daemonInstanceId|codebaseDaemonInstanceId|sshConfigHost`. Read `CreateSessionHostField.tsx`, `useCreateSessionPlacement.ts` 1-80 and `codebasePlacement.ts` 1-60.
27. Grepped `useCreateSessionSubmit.ts` and `createSessionRequest.ts` for the client and daemon fields. Read `hooks/useSessionAttachments.ts` 380-450 and grepped its client imports.
28. Listed `tddy-client-web/src` and its connections, and grepped `openTerminal|openRoomTerminalFeed`. Read `connections/types.ts` 1-140, `session.ts` 60-140 and `terminal.ts` 1-80.
29. Listed `tddy-web/src/app/rpc/connections` and `tddy-livekit-web/src/connections`. Read `localHost.ts` 1-60 and 395-440, `tddy-session-web/src/rpc/connections/hostServedSession.ts` 1-80, and `tddy-livekit-web/src/connections/sessionConnection.ts` 240-290.
30. Found and outlined `tddy-terminal-rpc/proto/terminal_session.proto`.
31. Listed `tddy-daemon/src` and `src/runtime`. Read `common_room_supervisor.rs` 1-80, 115-140 and 240-340, and grepped `runtime/integrations.rs`.
32. Read the `RequestTransport` enum in `tddy-rpc/src/message.rs` and listed `tddy-rpc` src/docs.
33. Grepped `impl PeerForwarder for|impl EligibleDaemonSource for|peer.testkit|InProcessPeers`: what test transports exist.
34. Read `tddy-daemon-rpc/tests/project_update_delete_acceptance.rs` 75-140 and checked the testkits for `PeerForwarder`.
35. Read `tddy-room-client/src/lib.rs` in full, `in_memory.rs` 1-40 and its `Cargo.toml`.
36. Outlined `tddy-rpc/src/transport.rs`, `client_engine.rs` and `bridge.rs`. Read `transport.rs` 1-60, `tddy-stdio/src/lib.rs` and `tddy-livekit-testkit/src/livekit_testkit.rs` 1-40.
37. Listed the WIP feature docs and grepped the todo files for livekit/peer/transport/remote. Printed every todo title that matched (457 todos in total).
38. Read 4 todos in full: `2026-11-05-tests-use-cheap-rpc…`, `2026-10-24-peer-forwarding-tests…`, `2026-10-21-tddy-daemon-binary-cannot-be-built-without…` and `2026-09-06-host-id-should-become…`.
39. Read `tddy-daemon-kernel/tests/common_room_forwarding.rs` 1-68 and `tddy-session-split/tests/split_room_hosting_port.rs` 600-640: the seed scripted peers.
40. Grepped the headings of `livekit-peer-discovery.md`, `remote-managed-worktree.md`, `terminal-sessions.md`, `session-room.md`, `web-terminal.md`, `session-drawer.md`, the satellite PRD, `rpc-multi-transport.md`, `projects-screen-multi-host.md` and `remote-codebase-mode.md`.
41. Read `livekit-peer-discovery.md` 1-75, and `terminal-sessions.md` 275-337 and 473-572.
42. Read `tddy-daemon-livekit/src/session_reach.rs` 1-80 and grepped `expose_session_terminal|expose_terminal`.
43. Read `session-drawer.md` 266-320 and `session-room.md` 6-45.
44. Read the satellite PRD 1-40 and `rpc-multi-transport.md` 1-60.
45. Read `remote-managed-worktree.md` 357-412 and the `SessionToolTransport` enum (`tddy-session-tool-client/src/lib.rs` 48-110).
46. Listed `tddy-livekit-rpc-web/src` and grepped its exports. Grepped crate descriptions and the `RpcClientTransport` implementations.
47. Grepped the daemon wiring (`peer_routing_over`, `LiveKitEligibleDaemonSource::new`, `DaemonCommonRoomConnector::new`) and web `connectSession(`. Read `useSessionAttachment.ts` 110-175, `useSessionsDrawerScreenActions.ts` 100-125 and `useSessionsDrawerActivation.ts` 140-175.

---

#### 2. Inspected files (with excerpts)

### Ports (LiveKit-free core)

- **`packages/tddy-daemon-extensions/Cargo.toml` lines 12-14**: "Nothing here may reach a LiveKit crate, transitively… `tests/livekit_is_a_satellite.rs` holds that line." Dependencies are only `async-trait`, `serde`, `serde_json`, `tddy-rpc` and `tokio`.

- **`packages/tddy-daemon-extensions/docs/extension-ports.md`** is the full port catalogue:
- `SessionReach` / `ReachedSession` (`arrived_over: tddy_rpc::RequestTransport`) / `OpenedRoom {room,url,server_identity}` / `ReachableSessions` / `NoSessionReach`
- `TerminalExposure` / `TerminalAddress {room, identity}` / `ExposedTerminal` / `NoTerminalExposure`
- `SessionStarter` and `LateBoundSessionStarter`
- `IntegrationStatusBoard`
- `SessionRooms` / `NoSessionRooms`
- `RoomTokenMinter` / `NoRoomTokens`
- `RoomBroadcast`, `RoomAdmission`, `RoomDirectory`
- `SplitRoomHosting` / `NoSplitRooms`

Host builder table (lines 199-221): `with_session_reach`, `with_terminal_exposure`, `with_session_rooms`, `with_room_broadcast`, `with_room_admission`, `with_room_directory`, `with_room_tokens`, `with_room_connector` (`tddy_room_client::RoomConnector` / `NoRoomTransport`), `with_split_rooms`, `with_session_delta_stores`.

`DaemonSessionHost::new(... peer_routing, ...)` takes `tddy_host_service::peer_routing::PeerRouting` (`PeerRouting::local_only(&config)` when there are no peers). `tddy-session-lifecycle/tests/host_without_integrations.rs` holds that a host with all null objects can start, connect to, resume and delete a session.

The satellite ratchet (lines 235-283):
- `LIVEKIT_FAMILY` = livekit, livekit-api, livekit-protocol, tddy-livekit, tddy-livekit-testkit, tddy-daemon-livekit, tddy-livekit-screen-capture, tddy-screenshare
- `PLUGINS` = the five tddy LiveKit crates
- `WIRING` = tddy-daemon, tddy-coder-cli, tddy-remote-git-repo-cli, tddy-session-sync-cli, tddy-tools-cli, tddy-rdp, tddy-vnc, tddy-e2e
- `CLIENT_LIBRARIES` = tddy-remote-git-repo, tddy-session-sync, tddy-tools

`[dev-dependencies]` are not counted. "No library crate names LiveKit": the `KNOWN_OFFENDERS` list shrank from 18 to none.

- **`packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` lines 25-67, 134**: the constants above, plus `LIVEKIT_PATHS = ["livekit::","livekit_api::","livekit_protocol::"]`.

- **`packages/tddy-daemon-extensions/src/peer_forwarder.rs`** (the peer transport port):
```rust
#[async_trait::async_trait]
pub trait PeerForwarder: Send + Sync {
    async fn is_connected(&self) -> bool;
    async fn call_unary(&self, peer_id:&str, service:&str, method:&str, body:Vec<u8>) -> Result<Vec<u8>, Status>;
    async fn call_server_stream(&self, peer_id:&str, service:&str, method:&str, body:Vec<u8>)
        -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status>;
}
pub struct NoPeerForwarding; // failed_precondition "no peer transport is wired on this daemon; cannot forward {service}/{method} to daemon {peer_id}"
```
The doc comment says it carries "one call to `daemon-{peer_id}`". The port has no bidi or client-stream methods.

- **`packages/tddy-daemon-extensions/src/terminal_exposure.rs`**: `TerminalAddress { room, identity }`; `TerminalExposure::expose(&self, at, terminal: Arc<dyn tddy_rpc::RpcService>, metadata_json: Option<String>) -> Result<ExposedTerminal, Status>`; `NoTerminalExposure` answers "no terminal exposure is wired: this daemon's terminals are reachable over its own connection only". The address is room- and identity-shaped, which is LiveKit vocabulary.

### Kernel (policy, transport-neutral)

- **`packages/tddy-daemon-kernel/src/peer_forwarding.rs`**:
- `PeerRoute { Local, Forward { peer_instance_id } }`.
- `classify_peer_route` (lines 35-66). Its error text is LiveKit-specific: "unknown or not connected daemon_instance_id … (configure livekit.common_room and ensure the peer is in the same LiveKit room)".
- `daemon_rpc_identity(instance_id)` (line 78) returns `format!("{}{}", tddy_service::RESERVED_DAEMON_IDENTITY_PREFIX, id)`, i.e. `daemon-{id}`.
- `session_room_name` (line 93) returns `session-{id}`.
- `PEER_FORWARD_TIMEOUT` and `PEER_FORWARD_STREAM_IDLE_TIMEOUT` are 30 s.
- `pub struct CommonRoom { forwarder: Arc<dyn PeerForwarder>, forward_timeout }` (line 126) with `forward_to_peer`, `forward_to_peer_within`, `forward_server_stream_to_peer` and `forward_server_stream_classified` (crate-private), all over `open_relayed_stream`.

- **`packages/tddy-daemon-kernel/src/peer_calls.rs`**: "Transport-neutral … re-exports them under their old `*_via_livekit` names". Typed forwards (lines 61-410): `forward_stream_start_session`, `forward_start_session_relaying` (no overall deadline; the idle timeout and keep-alives decide), `ForwardedStartFailure { PeerAnswered, PeerSilent }`, `forward_cancel_task`, `forward_delete_session`, `forward_stream_execute_tool`, the project mutations, staged attachments, and `forward_(stream_)read_host_document`. The service and method are strings, e.g. `"session.SessionService","StreamStartSession"`.

### Host-service (routing, LiveKit-free)

- **`packages/tddy-host-service/src/peer_routing.rs`**:
- `pub struct PeerRouting { config, eligible_daemon_source: Arc<dyn EligibleDaemonSource>, common_room: Option<CommonRoom> }`
- methods: `new`, `local_only`, `common_room_livekit_room()` (LiveKit name, neutral type), `classify_daemon_route` (FailedPrecondition), `classify_addressed_daemon_route` (InvalidArgument), `common_room_slot(rpc_name)` (error "this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)"), `rpc_served_by_peer<Req,Resp>`, `stream_served_by_peer`.

- **`packages/tddy-host-service/src/multi_host.rs` lines 10-45**: `DaemonInstanceId(String)` ("Stable identifier for a daemon instance in a shared LiveKit common room"), `EligibleDaemonInfo { instance_id, label }`, `trait EligibleDaemonSource { list_eligible_daemons; live_known_hosts (durable host id); async peer_project_entries(session_token) }`. `LocalOnlyEligibleDaemonSource` (line 89) holds two rows: the routing id and the durable host id.

### LiveKit plugin (`tddy-daemon-livekit`)

- **`livekit_peer_forwarder.rs`**: `pub type CommonRoomSlot = Arc<RwLock<Option<Arc<Room>>>>`. `LiveKitPeerForwarder` implements `PeerForwarder`. `peer_client` returns `LiveKitRpcClientFactory::for_room(room_arc).client(daemon_rpc_identity(peer_id))`. A stream relay task holds the client; `STREAM_RELAY_DEPTH` is 32, with a head-of-line warning.

- **`peer_routing.rs`**: `peer_routing_over(config, discovery: Option<LiveKitDiscoveryHandles>) -> PeerRouting` builds `CommonRoom::new(LiveKitPeerForwarder::new(handles.common_room_livekit_room), config)`, or `PeerRouting::local_only`.

- **`livekit_peer_discovery.rs`**:
- Header (1-70) covers the advertisement JSON in participant metadata, trust ("Anyone who can join the configured LiveKit room…"), the 500 ms resync, 2 s/6 s reconnect backoff, and set_metadata retries.
- `LiveKitDiscoveryHandles { eligible_daemon_source, common_room_livekit_room }` (105).
- `DaemonAdvertisement { instance_id, label, repos_base_path, max_attachment_bytes, sandboxed_codebase, signing_key_id, signing_public_key }` (112-148).
- `PeerDaemon { advertisement, host_id }` (211), `CommonRoomPeerRegistry` (381), `LiveKitEligibleDaemonSource` (638, implements `EligibleDaemonSource`), `aggregate_peer_project_entries`, `spawn_common_room_discovery_loop` (825), `livekit_common_room_connect_strings` (880).
- Line 1397 re-exports `tddy_daemon_kernel::peer_calls::{… forward_stream_start_session as forward_stream_start_session_via_livekit …}`.

- **`livekit_peer_transport.rs`**: `LiveKitPeerTransport` implements `tddy_credential_sync::PeerTransport` (attribute `tddy.credential_sync.advertisement`, service `tddy.credential_sync.CredentialSync/Send`). Its TODO says no receiving handler is registered yet.

- **`common_room_supervisor.rs`**:
- A daemon is in the common room twice: the RPC participant `daemon-{instance}` and the discovery participant under the bare id.
- Traits: `CommonRoomSupervisor::reconfigure` (40), `CommonRoomConnector::connect(target)` (122) and `ConnectedCommonRoom::disconnect` (129).
- `CommonRoomTarget` (59), `PeerDiscoveryHandles { registry, room_slot: Arc<RwLock<Option<Arc<livekit::Room>>>>, signing_key, status }` (241).
- `DaemonCommonRoomConnector { config, entries: Vec<tddy_rpc::ServiceEntry>, peer_discovery }` (254) serves the whole daemon RPC roster as `daemon_rpc_identity(local_instance_id)` through `serve_rpc_participant_until_shutdown`.
- TODO: "peer discovery is assembled at startup… a daemon that gains one at runtime… without discovering… until it is restarted".

- **`terminal_exposure.rs`**: `LiveKitTerminalExposure { url (public_url preferred), api_key, api_secret, ice_gathering }`. `expose()` mints a 24 h `TokenGenerator` token, calls `LiveKitParticipant::connect(url, token, terminal, room_options…)`, and republishes the session metadata every 30 s.

- **`session_reach.rs` lines 1-80**: a caller that arrives over LiveKit gets the room and terminal opened in its own time, and a failure fails its request. Any other arrival opens the room in the background. Also `livekit_integration_state`, `LIVEKIT_INTEGRATION_ID = "livekit"`, `LiveKitSessionReach { config, rooms: Arc<SessionRoomRegistry>, status }`.

### Daemon wiring

- `packages/tddy-daemon/src/runtime/integrations.rs` lines 8-125: `SupervisedCommonRoom`, `SessionRoomRegistry`, `SessionAdmissionRegistry`, `LiveKitSessionReach::new`, `LiveKitTerminalExposure::from_config`.
- `runtime/mount_host.rs:359`: `tddy_daemon_livekit::peer_routing::peer_routing_over(config, livekit_discovery)`.
- `runtime/mount.rs:155`: `LiveKitEligibleDaemonSource::new(`.
- `runtime/build_context.rs:155`: `DaemonCommonRoomConnector::new(`.

### Session start, routing and connect handlers (`tddy-agent-launch`, `tddy-session-lifecycle`)

- **`packages/tddy-agent-launch/src/svc_start_session_core.rs` 99-121** (unary `StartSession`): `classify_peer_route(&local_id, req.daemon_instance_id, &eligible_ids)`. `Forward` calls `forward_start_naming_local_source(&req, os_user, peer_instance_id)`. `Local` goes on to `classify_start_placement` and `start_here`.

- **`start_request_checks.rs` 318-347** (`forward_start_session`):
```rust
let slot = self.peer_routing.common_room_livekit_room().ok_or_else(|| Status::failed_precondition(
    "cannot forward StartSession: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)"))?;
let inner = tddy_daemon_kernel::peer_calls::forward_start_session_relaying(slot, &peer_instance_id, req, &|event| discarded.relay(event)).await…;
```

- **`session_coordinate_handlers.rs` 305-345** (`StreamStartSession`): authenticates first, then `peer_routing.classify_daemon_route(&req.daemon_instance_id)`. `Forward` calls `common_room_slot("StreamStartSession")` and `forward_stream_start_session(slot, &peer, &req)`, then pipes the events back through `family_proto_bridge::wire_same`.

- **`session_coordinate_handlers.rs` 180-227** (`ConnectSession`): `tell_the_reach(...)`, then `has_terminal_room` is true only for `SessionLaunch::TddyCoder`. CLI and workspace sessions get empty `livekit_room/url/server_identity`. tddy-coder sessions get `livekit_url` (public_url), `metadata.livekit_room` and `livekit_server_identity_for_session(instance, session_id)`. `ConnectSessionRequest` has no `daemon_instance_id`, so it is never forwarded. The client must call the owning daemon directly.

- **`tddy-session-lifecycle/src/connection_service/terminal_bridge_impl.rs` 34-70** (`expose_session_terminal`):
```rust
let Some(lk) = spawner::livekit_creds_from_config(&self.config) else { return Ok(()); };
let at = TerminalAddress { room: spawner::resolve_livekit_room_name(lk.common_room.as_deref(), session_id),
                           identity: spawner::livekit_server_identity_for_session(lk.daemon_instance_id.as_deref(), session_id) };
self.cli_manager.ensure_exposed_terminal(session_id, &at, self.terminal_exposure.as_ref())
```
It is called from `svc_reachable_sessions.rs:69-70` (`ReachableSessions::expose_terminal`). `ensure_exposed_terminal` lives at `tddy-cli-sessions/src/cli_session_manager/exposed_terminals.rs:70`, which also has `expose_terminal_to_livekit` (line 39).

- **`packages/tddy-spawn/src/spawner.rs`**:
- `LiveKitCreds { url, api_key, api_secret, common_room, daemon_instance_id }` (235).
- `resolve_livekit_room_name` (253) gives the common room or `daemon-{session_id}`.
- `livekit_server_identity_for_session` (269) gives `daemon-{instance}-{session}` or `daemon-{session}`.
- The tddy-coder spawn argv (885-893) passes `--livekit-url/--livekit-api-key/--livekit-api-secret/--livekit-room/--livekit-identity` unconditionally.
- `livekit_spawn_daemon_instance_id` (1280) and `livekit_creds_from_config` (1294).
- The crate has no LiveKit dependency but uses LiveKit vocabulary and configuration.

### Session host model and persistence

- **`packages/tddy-changeset/src/session_metadata.rs`** (`.session.yaml`):
- `livekit_room: Option<String>` (25)
- `codebase_daemon_instance_id` (93), with the note: "Persisted, unlike `SessionEntry.daemon_instance_id`, which is stamped at read time"
- `codebase_session_id` (97), `agent_daemon_instance_id` (103), `agent_session_id` (114)
- `ssh_config_host` (120): "OpenSSH `Host` alias… Absent = LocalShell… every exec-catalog tool runs there via RemoteShell"
- `managed_codebase` (125)

No field records which host owns a co-located session. Ownership is implied by which daemon's disk holds the file.

- **`packages/tddy-service/proto/session.proto`**:
- `ConnectSessionRequest{session_token, session_id}` and `ConnectSessionResponse{livekit_room=1, livekit_url=2, livekit_server_identity=3}` (144-153)
- `SessionEntry.daemon_instance_id=8` ("Owning daemon instance for multi-host selection"), `codebase_daemon_instance_id=29`
- `StartSessionRequest.daemon_instance_id=5` ("Empty or matching local instance = spawn locally"), `codebase_daemon_instance_id=32` ("the agent reaches it only via mcp__tddy-tools__* over LiveKit")
- `StartSessionResponse{session_id, livekit_room, livekit_url, livekit_server_identity, branch_conflict}` (772-781)
- `StartSessionEvent`, `StartTaskStarted.daemon_instance_id`, and `stack_parent_daemon_instance_id`/`orchestrator_daemon_instance_id`

- **`proto/terminal.proto`**: `TerminalService.StreamTerminalIO(stream TerminalInput) returns (stream TerminalOutput)`. The data has snapshot, start_offset and end_offset.
- **`proto/host.proto`**: `HostService.ListEligibleDaemons`, `ListKnownHosts`, `EligibleDaemonEntry{instance_id,label,is_local}`, `KnownHostEntry{… online, first_seen, last_seen, repos_base_path, max_attachment_bytes, is_local}`.
- **`proto/session_room.proto`**: `SessionRoomService.OpenSessionRoom`.
- **`proto/livekit.proto`**: `LiveKitService.StreamLiveKitRooms`.
- **`packages/tddy-terminal-rpc/proto/terminal_session.proto` lines 10-37**: `TerminalSessionService` with `StreamSessionTerminalIO` (bidi), `StreamTerminalOutput`, `SendTerminalInput`, `GetTerminalHistory`, `Start/Stop/ListTerminalSessions`, `ListTmuxSessions`, `ClaimTerminalControl`, `WatchTerminalControl`.

### RPC transport layer

- **`packages/tddy-rpc/src/message.rs` line 13**: `enum RequestTransport { InProcess, LiveKit, UnixSocket, Pipe, Http, Grpc, … }`, with a code-internal variant documented at lines 30-38.
- **`packages/tddy-rpc/src/transport.rs` 18-50**: `trait RpcClientTransport { call_unary, call_server_stream, call_client_stream, call_bidi_stream }`, plus the frame codec (`FrameKind`, `encode_frame`, `FrameDecoder`) for byte-stream transports.
- Implementations: `tddy-livekit/src/client.rs:304` (`RpcClient`), `tddy-stdio/src/client.rs:98` (`StdioRpcClient`), `tddy-session-tool-client/src/daemon_http_rpc.rs:52`.
- Server side: `tddy-rpc/src/bridge.rs` (`RpcService`, `ServiceEntry`, `MultiRpcService`, `RpcBridge`) and `tddy-livekit/src/participant.rs:95` (`LiveKitParticipant<S>`).
- **`packages/tddy-stdio/src/lib.rs`**: "stdio/IPC transport… A single duplex pipe pair carries both Request and Response frames… either side can call into the other". Exports `StdioRpcClient`, `StdioEndpoint`, `spawn_child_endpoint`, `ChildEndpoint`.
- Other transport crates (from their descriptions): `tddy-tauri-rpc` ("Webview-IPC RPC transport… over one duplex frame channel") and `tddy-connectrpc` (HTTP Connect).
- **`packages/tddy-room-client/src/lib.rs`** (client-side room port, LiveKit-free): `RoomAuth {Credentials, Token}`, `RoomJoin {url, room, identity, auth}`, `RoomConnectError {Room, PeerAbsent}`, `RoomDuplex`, `trait RoomSession { rpc() -> Arc<dyn RpcClientTransport>; open_duplex; subscribe(topic); peer_present }`, `trait RoomConnector::connect(join, peer_identity, wait)`, `NoRoomTransport`. `in_memory.rs` (feature `test-support`) has `InMemoryRoomConnector` and `ScriptedJoin {Joined(Arc<dyn RpcClientTransport>), PeerAbsent, Unreachable}`.
- **`packages/tddy-session-tool-client/src/lib.rs` 48-92**: `enum SessionToolTransport { SandboxIpc{socket_path}, DaemonUds{…}, DaemonHttp{…}, LiveKit{url, room, token, server_identity, session_id, session_token, daemon_instance_id}, IncompleteLiveKit{missing} }`. This is how a split agent reaches its remote codebase daemon's `ExecToolService`.

### Remote shell (SSH exec, already exists)

- **`packages/tddy-tool-engine/src/shell.rs`**:
- `trait Shell { kind() -> ShellKind{Local,Remote}; root(); read_to_string; write_string; run }`
- `LocalShell`; `RemoteShell { host_alias, remote_root }` runs `ssh -o BatchMode=yes -- <alias> <cmd>` through `tddy_core::run_ssh_batch`
- `session_shell(local_root, ssh_config_host)`: an empty alias gives `LocalShell`, otherwise `RemoteShell`, with no fallback
- The header says "Dispatch stays IPC/HTTP/LiveKit to the code-managing daemon"
- Tests: `tddy-tool-engine/tests/remote_shell_exec_acceptance.rs`, `tddy-session-lifecycle/tests/ssh_exec_start_acceptance.rs`, `tddy-session-worktree/tests/ssh_exec_worktree.rs`, `tddy-worktree-service/tests/remote_git_ssh_serve_acceptance.rs`
- Web: `tddy-session-create-web/src/CreateSessionSshConfigSelect.tsx`

### Web: host picker, start, attach, terminal

- **`packages/tddy-session-create-web/src/CreateSessionHostField.tsx`**: a `<select data-testid="create-session-host-select">` over `daemons: DaemonHost[]`. It returns null when `daemons.length === 0` ("shown only when the common room advertises daemons").
- **`useCreateSessionPaneState.tsx` 122-123**: `daemonInstanceId` defaults to `projectHome?.daemonInstanceId || initialValues?.daemonInstanceId || selectedInstanceId || ""`.
- **`useCreateSessionPlacement.ts`**: `codebaseDaemonInstanceId` (split placement), `sshConfigHost`, and `canChooseCodebaseHost = agentCanSplit && managedCodebase && daemons.length > 0`.
- **`codebasePlacement.ts`**: `CodebasePlacementChoice = "none" | "sandbox" | "managed" | "sandboxedCodebase"`.
- **`createSessionRequest.ts` 229-232**: `codebaseDaemonInstanceId: split ? … : ""`, `sshConfigHost: features.managedWorkflow && managedCodebase ? sshConfigHost : ""`.
- **`hooks/useSessionAttachments.ts` 390-430**: `for await (const event of client.streamStartSession(request))`. The `client` is the form's `Client<typeof SessionService>`, i.e. the selected host's client; the request carries `daemonInstanceId`. Comment: "the LiveKit transport accepts a signal for server-streaming calls and never reads it".
- **`packages/tddy-session-web/src/useSessionAttachment.ts` 128-136**: `connectSession(sessionId, token, host: HostConnection)` calls `client.connectSession(...)` and then `host.openSession(sessionId, hint)`.
- **`useSessionsDrawerActivation.ts` 155-160**: `owningHostForSession(session, selectedInstanceId)` then `connectHost(owner)`, so an attach goes to the owning daemon.
- **`useSessionsDrawerScreenActions.ts` 114-120** (`handleSessionCreated`): `connectHost(selectedInstanceId)`. Right after creation it attaches to the **selected** host, not to the host picked in the form. Since `ConnectSession` does not forward, this probably fails when a session is created on a peer. I did not verify this further; the liveness effect may re-attach to the owner later.
- **`packages/tddy-client-web/src/connections/types.ts`**: `ConnectionCapability = "rpc"|"media"|"presence"`, `ConnectionStatus`, `interface HostConnection { hostId, providerId, status, error, capabilities, clientFor, transport, openSession(sessionId, hint) }`, `interface ConnectionProvider { id; connectHost(hostId) }`. The PRD reference is `docs/dev/1-WIP/2026-09-05-optional-livekit-connection-model-prd.md` (no longer present in 1-WIP).
- **`connections/session.ts` 60-102**: `SessionConnection { …, capabilities, clientFor, transport, close, openTerminal(options): TerminalFeed }`, `SessionAttachmentState`.
- **`connections/terminal.ts`**: `TerminalFrame`, `TerminalStream`, `TerminalHistoryChunk`, `TerminalHistoryFetcher`, `TerminalFeed`.
- **`packages/tddy-livekit-web/src/connections/sessionConnection.ts` 255-265** (`openLiveKitSession`, `openTerminal`): `openRoomTerminalFeed({ room, serverIdentity: targetIdentity, terminal: clientFor(TerminalService), host: support.hostClientFor(TerminalSessionService), sessionId, options })`. Bytes come from the room participant; history comes from the host daemon.
- **`packages/tddy-session-web/src/rpc/connections/hostServedSession.ts`** (`openHostServedSession(host, hint)`): clients and transport come from the host connection, and `openTerminal` uses `openDaemonTerminalFeed` over the host's `TerminalSessionService`.
- **`packages/tddy-web/src/app/rpc/connections/localHost.ts`**: `IPC_PROVIDER_ID = "ipc"`, `IPC_CAPABILITIES = {"rpc"}`, `createIpcConnectionProvider` (registered first, claims exactly one host). `openTerminal` uses `openDaemonTerminalFeed` over `host.clientFor(TerminalSessionService)`.
- **`packages/tddy-livekit-rpc-web/src/transport.ts`**: `RoomRpcRegistry` (111), `LiveKitTransport implements Transport` (269), `createLiveKitTransport` (402), `LiveKitTransportFactory` (413).
- `packages/tddy-livekit-web/src/connections/liveKit.tsx` holds `LiveKitConnectionProvider` (per `host-connections.md`; I did not open the file).

### Test transports

- **`packages/tddy-daemon-kernel/tests/common_room_forwarding.rs` 1-68**: `AScriptedPeer`, an in-memory `PeerForwarder` with `Answer { Bytes, Failure, Never, Frames, OpenFailure }` that records the calls it carried.
- **`packages/tddy-session-split/tests/split_room_hosting_port.rs` ~600-640**: `ACodebaseHost` implements `PeerForwarder` (handles CancelTask and StreamStartSession).
- **`packages/tddy-daemon-rpc/tests/project_update_delete_acceptance.rs` 75-140**: `InProcessPeers` implements `PeerForwarder`. It dispatches decoded requests by hand to real in-process daemon services, stamped `RequestMetadata::over(RequestTransport::LiveKit)`, and anything else gets `unimplemented "the in-process peers do not serve …"`.
- **`packages/tddy-livekit-testkit/src/livekit_testkit.rs`**: a testcontainers LiveKit server (or the `LIVEKIT_TESTKIT_WS_URL` env var), image pinned in `.config/livekit-server.image`, dev key `devkey`/`secret`.
- `packages/tddy-connectrpc-testkit/src/{backend.ts,index.ts}` is a TypeScript Connect backend testkit (not opened).

### Docs read

- **`docs/ft/web/daemon-selector-livekit-rpc.md`**:
- HTTP `/rpc` is same-origin only because of CORS, so peers are reached over a "host connection"; LiveKit is the provider today.
- Bootstrap that stays on HTTP: `GET /api/config` and `TokenService.generateToken/refreshToken`.
- Per-session RPC targets `daemon-{instanceId}-{sessionId}`.
- Discovery participant is the bare id; the RPC participant is `daemon-{id}`.
- `CreateSessionPane` Host `<select>` threads `daemonInstanceId` into `StartSession` and `ListProjectBranches`.
- The desktop uses `createIpcConnectionProvider`.
- **`packages/tddy-web/docs/host-connections.md`**: the model, precedence by registration order, observable registry, the `HostConnectionGate` table (it reads `providerId` to tell LiveKit apart). Current limits: "`idle` and `error` have no producer"; "The host directory is still the common room."
- **`packages/tddy-web/docs/session-connections.md`**: `attachmentHintFromReply` / `capabilitiesForHint` (a room gives `{rpc,media,presence}`, none gives `{rpc}`). It describes the two implementations, how `openTerminal` gets bytes and history, the lifetime rules, and `liveKitRoomOf`.
- **`packages/tddy-web/docs/host-directory.md`**: `HostDescriptor`/`HostDirectorySource`/`HostDirectory`; sources are `livekit` (common room participants) and `serving` (`/api/config` `daemon_instance_id`); `useHostPresence(hostId): Room | null`.
- **`docs/ft/daemon/livekit-peer-discovery.md` 1-75**: config (`livekit.enabled`, `common_room`, `peer_forward_timeout_secs`, `daemon_instance_id`), the "Forwarding transport" paragraph (behind `PeerForwarder`), the trust model (room membership, plus a signature on forwarded session tokens), StartSession routing. "ResumeSession, ConnectSession, DeleteSession, and SignalSession remain owned by the daemon that holds the session."
- **`docs/ft/daemon/terminal-sessions.md` 275-337 and 473-572**:
- The daemon participant serves bootstrap.
- For tddy-coder sessions, the coder participant `daemon-{instanceId}-{sessionId}` serves session-scoped RPC including 8 of the 9 terminal methods.
- The daemon serves everything for CLI and workspace sessions.
- Room-carried terminals use `terminal.TerminalService/StreamTerminalIO` with snapshot and offsets; history comes from the host's `GetTerminalHistory`.
- One PTY-over-RPC surface lives in `tddy-terminal-rpc`.
- `pty_relay` has 4 dispatch modes (HTTP or LiveKit via `OpenSessionRoom`).
- **`docs/ft/daemon/session-room.md` 6-45**: room `session-{id}` hosted by the facilitating daemon as `daemon-{instance}`. Starting a session does not open its room, and a local attach never waits on LiveKit. Roles: facilitating daemon vs codebase daemon.
- **`docs/ft/web/session-drawer.md` 266-320**: cross-host active sessions come from LiveKit presence (`daemon-<instanceId>-<sessionId>` participants) via `parseSessionParticipantIdentity`. There is no `ListSessions` fan-out. Without presence a footnote shows. `SessionManager` lives in `tddy-session-web`.
- **`docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-integration.md` 1-40**: "LiveKit is a satellite… becomes a plugin: the daemon's core defines extension ports".
- **`docs/ft/coder/rpc-multi-transport.md` 1-60**: the stdio/IPC transport for `tddy-rpc`. "Same user code, either transport"; multiplexed and bidirectional; Rust only.
- **`docs/ft/daemon/remote-managed-worktree.md` 357-412**: the `SessionToolTransport::LiveKit` variant, and `StreamExecuteTool` rides `forward_server_stream_to_peer`.

### Todos read in full

- **`docs/dev/todo/2026-11-05-tests-use-cheap-rpc-livekit-only-for-compatibility.md`**:
- The rule: behaviour tests use in-memory, stdio or IPC; LiveKit is tested only in a dedicated compatibility package with one contract suite per port; media goes behind a port.
- It measured 43 test files using the LiveKit testkit. `tddy-daemon` has 11, including `multi_host_acceptance`, `session_attach_cross_host_acceptance` and `remote_managed_worktree_cross_host_acceptance`; `tddy-daemon-livekit` has 9; `tddy-livekit` 8; `tddy-e2e` 4; and so on.
- Media (`tddy-screenshare`, `tddy-livekit-screen-capture`, `tddy-screen-sharing`, `tddy-vnc`) is LiveKit-shaped.
- Proposed fix: promote `AScriptedPeer`/`ACodebaseHost` into a shared testkit crate (an in-memory `PeerForwarder`, a session-room transport, a stdio/IPC adapter).
- **`docs/dev/todo/2026-10-24-peer-forwarding-tests-use-livekit-below-the-binary-boundary.md`**: wants two daemons' service stacks wired through an in-process `PeerForwarder`, and the `*_via_livekit` re-exports dropped.
- **`docs/dev/todo/2026-10-21-tddy-daemon-binary-cannot-be-built-without-the-livekit-plugin.md`**: `tddy-daemon` depends on and wires `tddy-daemon-livekit` unconditionally. LiveKit types appear in 7 of the 11 runtime stage files, `tasks.rs`, `runtime.rs` (`PeerDiscoveryHandles`) and `credential_sync.rs` (`livekit::Room`). The proposed fix is a default-on `livekit` feature.
- **`docs/dev/todo/2026-09-06-host-id-should-become-a-field-of-daemonadvertisement.md`**: the durable `host_id` rides beside the advertisement via `serde(flatten)`.

---

#### 3. Grep / glob log (pattern → scope → notable hits)

- `livekit` in `packages/*/Cargo.toml` hit 22 crates. The non-plugin, non-wiring ones only have dev-dependencies or comments:
- `tddy-daemon-auth` (dev: tddy-daemon-livekit, tddy-livekit, livekit-api)
- `tddy-daemon-rpc` (dev, lines 71/93-96)
- `tddy-session-lifecycle` (dev, 137-140)
- `tddy-worktree-service` (dev, 44-45)
- `tddy-screen-sharing:47` and `tddy-session-files:56` (`tddy-livekit`; I did not check whether these are dev-dependencies)
- `tddy-sandbox-app:50` (optional `livekit` feature)
- `tddy-credential-sync`, `tddy-sandbox-darwin`, `tddy-session-sync` (comments only)
- `peer.testkit|satellite` in Cargo.toml: only the ratchet comments (`tddy-daemon-extensions:12`, `tddy-telegram-control:46`). There is no peer-testkit crate.
- `forward_start_session_relaying|forward_stream_start_session|classify_daemon_route|PeerRoute::Forward` in packages (non-test). Hits:
- `tddy-session-split/src/split_forward_stream.rs:46`
- `tddy-session-agents/src/svc_provision_agent_clone.rs:98`
- `tddy-agent-launch/src/session_coordinate_handlers.rs:315,326`, `start_request_checks.rs:335`, `svc_start_session_core.rs:115`
- `tddy-host-service/src/service/peer_forwards.rs:54,84,116`
- `tddy-session-lifecycle/src/connection_service/svc_activity_ports.rs:177,284,312` (line 312 refuses with `unimplemented` for streams addressed at a peer)
- `svc_peer_routed_session_files.rs:101`
- `tddy-daemon-rpc/src/project/coordinate_route.rs:42`, `exec_tool/ports.rs:120`
- `tddy-session-files/src/svc_materialize_staged_attachment.rs:61,207`
- `livekit_room|livekit_url|livekit_server_identity` in session-lifecycle, agent-launch and cli-sessions. Hits:
- `handler_state.rs:64` (`common_room_livekit_room`), `terminal_bridge_impl.rs:39-40`
- `svc_resume_session.rs:116-222`, `session_coordinate_handlers.rs:200-227,403`
- `svc_*sandboxed_cli_harness_session.rs` (empty fields), `svc_start_session_core.rs:486`
- `ensure_exposed_terminal` hits `tddy-cli-sessions/src/cli_session_manager/exposed_terminals.rs:70` and tests `exposed_terminals.rs` and `pty_terminal_service_snapshot.rs`.
- `pub fn livekit|resolve_livekit_room_name|LiveKitCreds|--livekit` in `tddy-spawn/src` hit `spawner.rs:235,253,269,885-893,1280,1294`.
- `struct SessionMetadata` hits `tddy-changeset/src/session_metadata.rs`, `tddy-testing-commons/src/builders.rs`, `tddy-coder/src/session_participant/metadata_publisher.rs`, `tddy-changeset/src/session_participant_metadata.rs`.
- `RemoteShell|LocalShell` hits `tddy-tool-engine/src/shell.rs` (55-255), `lib.rs:40`, and the tests and web files listed in section 2.
- `daemonInstanceId|codebaseDaemonInstanceId|sshConfigHost` in `tddy-session-create-web/src` hits `CreateSessionHostField.tsx`, `useCreateSessionPaneState.tsx:122-290`, `useCreateSessionSubmit.ts:147`, `createSessionRequest.ts:60,98-99,128-232`, `CreateSessionSshConfigSelect.tsx:54,75`, `useCreateSessionStack.ts`, `useCreateSessionModel.ts`, `codingAgentMetadata.ts`.
- `startSessionStreamed` hits `hooks/useSessionAttachments.ts:124,390,448`.
- `openTerminal|openRoomTerminalFeed` hits `tddy-client-web/src/connections/session.ts:102`, `tddy-livekit-web/src/connections/roomTerminalFeed.ts:254`, `sessionConnection.ts:31,255`, `tddy-web/src/app/rpc/connections/localHost.ts:409`.
- `openHostServedSession` hits `tddy-livekit-web/src/connections/liveKit.tsx`, `tddy-session-web/src/rpc/connections/hostServedSession.ts`, `tddy-web/src/app/rpc/selectedDaemon.tsx`, `localHost.ts`.
- `TerminalSessionService` proto hits `packages/tddy-terminal-rpc/proto/terminal_session.proto`.
- `impl PeerForwarder for` gives 12 files:
- `tddy-session-split/tests/split_room_hosting_port.rs:623`
- `tddy-daemon-livekit/src/livekit_peer_forwarder.rs:69` (production)
- `tddy-daemon-sandbox/tests/task_service_peer_cancel.rs:29`
- `tddy-daemon-kernel/tests/peer_calls.rs:56`, `common_room_forwarding.rs:69`
- `tddy-session-lifecycle/tests/remote_managed_worktree_acceptance.rs:625`, `workspace_seed_admission_integration.rs:91`
- `tddy-daemon-extensions/src/peer_forwarder.rs:45` (null object)
- `tddy-daemon-rpc/tests/project_home_remote_targets_acceptance.rs:91`, `project_update_delete_acceptance.rs:84`, `remote_codebase_clone_placement_acceptance.rs:232`
- `impl EligibleDaemonSource for` gives about 20 hits, mostly test fakes in host-service, session-lifecycle and daemon-rpc tests. Production: `LocalOnlyEligibleDaemonSource` (`multi_host.rs:120`) and `LiveKitEligibleDaemonSource` (`livekit_peer_discovery.rs:676`).
- `impl RpcClientTransport for` (non-test) hits `tddy-session-tool-client/src/daemon_http_rpc.rs:52`, `tddy-stdio/src/client.rs:98`, `tddy-livekit/src/client.rs:304`.
- `peer_routing_over|LiveKitEligibleDaemonSource::new|DaemonCommonRoomConnector::new` in `tddy-daemon/src` hits `runtime/mount_host.rs:359`, `runtime/mount.rs:155`, `runtime/build_context.rs:155`.
- `connectSession(` in the web hits `tddy-session-web/src/useSessionsDrawerActivation.ts:161,216`, `useSessionsDrawerScreenActions.ts:120`, `useSessionAttachment.ts:50,132`, `SessionChildRuntime.tsx:74`.
- Todo titles matching livekit/peer/transport/host/remote: about 80 matches. The most relevant beyond the four read in full:
- `2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline`
- `2026-08-15-livekit-connect-streamer-duplication-still-outstanding`
- `2026-08-15-remote-git-repo-over-livekit-deliberate-gaps`
- `2026-08-13-tddy-coder-remote-never-completes-a-session-bootstrap`
- `2026-09-06-from-the-2026-09-06-optional-livekit-stack-evaluation`
- `2026-09-10-tddy-livekit-depends-on-tddy-service-for-two-call-sites`
- `2026-09-12-session-agent-clone-is-two-daemons-in-one-module` ("drags LiveKit into a crate that says it has none")
- `2026-10-01-session-tool-client-repeats-its-transport-selection-three-times`
- `2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits`
- `2026-10-23-host-gate-other-screens`
- `2026-10-23-session-status-streams-do-not-reconnect`
- `2026-10-25-remote-codebase-two-daemon-suite-not-run`
- `2026-10-25-web-host-terminal-feed-leftovers-after-snapshot`
- `2026-10-25-forwarded-start-cancel-reports-left-in-place-on-the-caller`
- `2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools`
- `2026-11-07-ssh-agent-launch-remote-pointer-owner`
- `2026-11-13-livekit-failed-join-leaks-its-session`
- `2026-11-14-node-session-not-listed-reads-when-its-stack-is-on-another-host`
- `2026-09-15-rpcservice-has-no-cancellation-surface`

---

#### 4. Findings

### A. End-to-end path of a remote session today

1. **Host list (web).** `SelectedDaemonProvider` merges two host directory sources:
 - `livekit`: common-room participants whose metadata parses as a `DaemonAdvertisement`
 - `serving`: `/api/config` `daemon_instance_id`

 `useSelectedDaemon().daemons` feeds `DaemonSelector` and `CreateSessionHostField`. The Host select only renders when `daemons.length > 0`. "Remote vs local" is not a separate choice: the user picks a `daemonInstanceId`, and it is remote when it differs from the serving daemon's id. Split placement additionally picks `codebaseDaemonInstanceId`. That option is only offered when the agent spec declares `split_host`, managed codebase is on, and daemons exist. There is also `sshConfigHost` (SSH exec target, no daemon on the far side).

2. **Which client is called.** The form's `SessionService` client belongs to the **selected** host's `HostConnection`:
 - from a browser, a `LiveKitConnectionProvider` connection, i.e. ConnectRPC over the LiveKit data channel to participant `daemon-{selected}` via `LiveKitTransport`
 - from desktop IPC, the `ipc` provider for the local host
 - The serving daemon itself over HTTP `/rpc` only through `daemonTransport.ts` (same origin).

 It calls `streamStartSession(request with daemonInstanceId)`.

3. **Daemon handler.** In `tddy-agent-launch`, `StreamStartSession` and unary `StartSession` call `PeerRouting::classify_daemon_route` / `classify_peer_route(local, requested, eligible_ids)`:
 - An empty or local id runs `start_here`.
 - An eligible peer id is forwarded with `peer_calls::forward_stream_start_session` / `forward_start_session_relaying` through `CommonRoom`, the `PeerForwarder` port and `LiveKitPeerForwarder`.
 - The LiveKit forwarder makes a data-channel RPC (`tddy_livekit::RpcClient` via `LiveKitRpcClientFactory::for_room`) to `daemon-{peer}` in the common room, where the peer's `DaemonCommonRoomConnector` serves the full daemon RPC roster (`Vec<ServiceEntry>`).
 - Events relay back. The peer creates the session on its own disk.

4. **Attach.** The web calls `ConnectSession` on the **owning** daemon directly (`owningHostForSession` → `connectHost(owner)`). `ConnectSessionRequest` has no daemon id and is never peer-forwarded. The reply is `{livekit_room, livekit_url, livekit_server_identity}`:
 - **tddy-coder sessions** get room coordinates (the common room or `daemon-{session}`; identity `daemon-{instance}-{session}`), and the coder process itself is a LiveKit participant spawned with `--livekit-*` args.
 - **CLI and workspace sessions** get empty fields.

 `attachmentHintFromReply` and `capabilitiesForHint` turn the reply into a hint and capabilities, then `host.openSession(...)`:
 - With a room it is `openLiveKitSession`: a second room join, observer identity `web-traffic-*`, and token refresh.
 - Without a room it is `openHostServedSession`, which routes over the host connection.

5. **Terminal bytes.**
 - A **CLI session on a remote host**: PTY (`PtyHandle`) → the remote daemon's `DaemonTerminalSessionStore` / `TerminalSessionService` (`tddy-terminal-rpc`) → served on `daemon-{peer}` in the common room → LiveKit data channel → the browser's `LiveKitTransport` → `openDaemonTerminalFeed`. The local or serving daemon is not in the byte path; the browser talks to the peer directly.
 - A **tddy-coder session**: the coder process's participant `daemon-{instance}-{session}` serves `terminal.TerminalService/StreamTerminalIO`. `openRoomTerminalFeed` takes bytes from it and history from the host's `GetTerminalHistory`.
 - **Also:** on a LiveKit-arrived reach, `SessionReach` calls `ReachableSessions::expose_terminal`, which calls `expose_session_terminal`, then `CliSessionManager::ensure_exposed_terminal`, then the `TerminalExposure` port, then `LiveKitTerminalExposure` (a `LiveKitParticipant` serving `pty_terminal_service(handle)` as `daemon-{instance}-{session}` in the common room or lobby, republishing metadata every 30 s). This is what Telegram attach hints, `pty-relay --livekit-url` and cross-host presence use.
 - **Daemon to daemon there is no terminal relay.** `PeerForwarder` has unary and server-stream only (no bidi), and `svc_activity_ports.rs:312` answers `unimplemented` for long-lived activity streams addressed at a peer.

6. **Cross-host visibility.** Live remote sessions appear in the drawer only through LiveKit presence (`daemon-<instanceId>-<sessionId>` participants). There is no `ListSessions` fan-out.

### B. Abstraction seams that already exist

**Rust, daemon side.** The daemon-side seams below have null objects:
- `PeerForwarder` (`NoPeerForwarding`), wrapped by the kernel's `CommonRoom` (deadline and idle policy)
- `PeerRouting` (`local_only`), with the `EligibleDaemonSource` trait (`LocalOnlyEligibleDaemonSource`)
- `TerminalExposure`, `SessionReach`/`ReachableSessions`, `SessionRooms`, `RoomTokenMinter`, `RoomBroadcast`, `RoomAdmission`, `RoomDirectory`, `SplitRoomHosting`
- `CommonRoomConnector`/`ConnectedCommonRoom`/`CommonRoomSupervisor`. These sit inside the LiveKit plugin crate, but they are a lifecycle seam that is tested without a server.
- `tddy_credential_sync::PeerTransport`

**Rust, client side.** `tddy_room_client::RoomConnector`/`RoomSession` (`NoRoomTransport`, `InMemoryRoomConnector`); `SessionToolTransport` (`SandboxIpc`/`DaemonUds`/`DaemonHttp`/`LiveKit`); `tddy_tool_engine::Shell` (`LocalShell`/`RemoteShell` over SSH).

**RPC core.** `tddy_rpc::RpcClientTransport`, `RpcService`/`ServiceEntry`/`MultiRpcService`, and the frame codec. Its implementations:
- `tddy-livekit` (`RpcClient`, `LiveKitParticipant`)
- `tddy-stdio` (pipe or UDS duplex, bidirectional)
- `tddy-tauri-rpc` (webview IPC)
- `tddy-connectrpc` (HTTP)

`RequestTransport { InProcess, LiveKit, UnixSocket, Pipe, Http, Grpc, … }` stamps each request with the transport it arrived on.

**Web.** `ConnectionProvider` → `HostConnection` → `SessionConnection` (`openTerminal` → `TerminalFeed`), capability gating (`rpc|media|presence`), `HostDirectorySource`. There are two providers: `livekit` (`tddy-livekit-web`) and `ipc` (`tddy-web/src/app/rpc/connections/localHost.ts`).

### C. Where LiveKit is still assumed

- **Proto, persistence and API names:**
- `ConnectSessionResponse` and `StartSessionResponse` have `livekit_room/livekit_url/livekit_server_identity`
- `SessionMetadata.livekit_room`
- `PeerRouting::common_room_livekit_room()`
- `DaemonInstanceId` is documented as "in a shared LiveKit common room"
- **Identity and addressing are a LiveKit participant scheme:** `daemon_rpc_identity` = `daemon-{id}`, `livekit_server_identity_for_session` = `daemon-{id}-{session}`, `TerminalAddress {room, identity}`, `session_room_name` = `session-{id}`, `resolve_livekit_room_name`.
- **Error strings** tell users to configure `livekit.common_room`: `classify_peer_route`, `common_room_slot` and `forward_start_session`.
- **`tddy-spawn`** builds the tddy-coder argv with `--livekit-url/api-key/api-secret/room/identity` unconditionally. The tddy-coder session type's terminal is therefore only reachable remotely through a LiveKit room.
- **`ConnectSession`** has room coordinates only for `SessionLaunch::TddyCoder`, and needs `livekit.url` (`Status::internal("LiveKit URL not configured")`).
- **The `tddy-daemon` binary** links and wires `tddy-daemon-livekit` unconditionally (todo 2026-10-21). LiveKit types appear in `runtime/{identity,integrations,mount,mount_host,mount_entries,mount_socket,mount_accounts}.rs`, `tasks.rs`, `runtime.rs` and `credential_sync.rs`.
- **Remote host discovery** is only `LiveKitEligibleDaemonSource` (common-room participant metadata). On the web the host directory's only remote source is `livekit`, and cross-host session liveness is LiveKit presence.
- **Only LiveKit carries "remote" in production.** On the daemon side, `PeerForwarder` has only `LiveKitPeerForwarder`; no stdio, SSH or HTTP peer forwarder exists. On the web side, the only remote-capable provider is LiveKit (IPC claims only the local host). `HostConnectionGate` reads `providerId` to special-case LiveKit.
- **The plugin re-exports kernel calls under `*_via_livekit` names** (`livekit_peer_discovery.rs` ~1397).
- **`PeerForwarder` has no bidi stream,** so a remote interactive terminal cannot be relayed daemon to daemon. Today it only works because the browser joins the same LiveKit room as the peer.

### D. Test transports, current state

- **No shared peer-testkit crate.** Each suite writes its own in-memory `PeerForwarder` (12 files, listed in section 3). Examples: `AScriptedPeer` (kernel), `ACodebaseHost` (session-split), `InProcessPeers` (daemon-rpc, dispatches to real services and stamps `RequestTransport::LiveKit`). There are about 20 `EligibleDaemonSource` fakes.
- **`tddy-room-client`** has the `in_memory` feature `test-support` with `InMemoryRoomConnector`.
- **`tddy-stdio`** gives a real cross-process duplex transport (used by tddy-coder/demo `--stdio`, sandbox-runner `--stdio`, and the tddy-tools sandbox IPC over UDS).
- **`tddy-livekit-testkit`** is the Docker LiveKit server. 43 test files still use it (per the 2026-11-05 todo). `.config/rust-e2e.filterset` keeps 99 binaries off the unit leg.
- **The satellite ratchet** is `tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` (plus `telegram_is_a_satellite.rs`). `tddy-session-lifecycle/tests/host_without_integrations.rs` proves a host with every port at its null object works.
- **Intended direction** (todos 2026-11-05 and 2026-10-24): a shared testkit (in-memory `PeerForwarder`, session-room transport, stdio/IPC adapter); cross-host suites moved onto it; one LiveKit compatibility package with a contract suite per port; and a media port for screen sharing and VNC.

### E. Relevant docs not opened in this pass

`packages/tddy-daemon-livekit/docs/{livekit-service.md, placement-and-admission.md, session-room.md}`, `packages/tddy-daemon-kernel/docs/daemon-kernel.md`, `packages/tddy-room-client/docs/room-client.md`, `packages/tddy-web/docs/{host-fan-out.md, local-host-ipc.md, capability-gating.md}`, `packages/tddy-terminal-rpc/docs/terminal-session-service.md`, `packages/tddy-rpc/docs/request-transport.md`, `docs/ft/daemon/{auth-livekit-services.md, remote-codebase-mode.md, host-worktree-services.md}`, `docs/ft/web/{projects-screen-multi-host.md, hosts-screen.md, livekit-wiring-package.md, client-ports-package.md}`, `docs/ft/coder/{session-participant-rpc.md, grpc-remote-control.md}`, and the full text of `remote-managed-worktree.md` and `web-terminal.md` (only their headings were read).
## Exploration 2: accounts, credentials at rest, Hosts list — 2026-11-15

**Agent**: Explore subagent
**Scope**: accounts/identity model, vault, provider accounts, Hosts screen, backlog entries

### Discovery dump: Accounts / identity model, credentials at rest, Hosts list (for a "Coder account": CLI + URL + token)

Repo root: `/Users/mantasi/Code/tddy-coder/.worktrees/plan-remote-shell-transports` (all paths below are relative to it unless absolute). Branch HEAD `cb64e89a3`. The only untracked file is the parent's `docs/dev/1-WIP/2026-11-15-remote-shell-transports-whole-work-initial-discovery.md`.

---

#### 1. Sequence (chronological)

1. `ls packages/`, `ls docs/ docs/ft docs/dev docs/dev/todo`: map the layout. This turned up the crates `tddy-accounts`, `tddy-credentials`, `tddy-credential-sync`, `tddy-users-file`, `tddy-daemon-auth`, `tddy-github`, `tddy-hosts-web` and `tddy-host-service`.
2. grep `auth_storage|INSTALL_AUTH_STORAGE_DIR` across `packages docs scripts` (*.rs, yaml, ts, tsx, sh, md, toml, nix): find where the vault directory is configured and consumed. The first attempt failed on a zsh glob and was re-run with quoted `--include`.
3. `find` over the 7 account-related packages: list their sources, docs and tests.
4. Read `packages/tddy-accounts/docs/accounts-service.md` and `provider-accounts.md`, then `wc -l` on the `src/*.rs` files: the service contract.
5. Read `packages/tddy-accounts/src/lib.rs` and `src/provider_account.rs` (lines 1-230): exports and the preset table.
6. Read `packages/tddy-accounts/docs/account-linking.md`, `github-identity-resolution.md`, `src/linking.rs` (1-120) and `src/local_gh.rs`: the linking ports and the Local gh model.
7. `ls packages/tddy-service/proto/`, then `grep -n ""` over `accounts.proto`: the wire contract.
8. Read `packages/tddy-credentials/docs/credential-store.md` (20-140), `src/record.rs` (79-110) and `packages/tddy-accounts/src/store.rs`; grep `pub fn|struct|enum` in `tddy-credentials/src/lib.rs` and `sessions.rs`: the record model and the vault registry.
9. Read `packages/tddy-daemon-kernel/src/config.rs` (1-240): the `DaemonConfig` fields.
10. Read `packages/tddy-users-file/README.md` and grep its `lib.rs` API: what `users.yaml` holds.
11. grep `struct GitHubConfig|KeyringConfig|ScreenSharingConfig` in `tddy-daemon-kernel/src/`, then read `config/access.rs` (1-50) and `config/sections.rs` (40-110).
12. `ls` the repo root yaml files; grep the top-level keys `users|github|auth_storage|keyring|…` in `dev.daemon.yaml`, `daemon.yaml.production` and `dev.desktop.yaml`.
13. Read `packages/tddy-daemon-kernel/src/live_users.rs` (1-80) and grep `users.yaml` in kernel `src/`.
14. grep `build_accounts_entry|accounts_service_over|with_linking|with_route_support|HarnessRouteSupport|HttpProviderModelProbe` (non-test): where the daemon wires the service. `ls packages/tddy-daemon/src`.
15. `ls docs/ft docs/ft/web docs/ft/daemon`.
16. Read `packages/tddy-daemon/src/runtime/mount_accounts.rs` (whole file), `account_linking.rs` (240-320) and `provider_route_support.rs` (1-55).
17. Read `docs/ft/web/accounts-screen.md`.
18. `find` for account, settings, provider and identity files in the web packages.
19. Read `packages/tddy-workspace-web/docs/accounts-screen.md` and `wc -l` the accounts and models components.
20. Read `packages/tddy-web/src/components/accounts/AccountsAppPage.tsx` and `packages/tddy-workspace-web/src/accounts/AccountsScreen.tsx` (1-80).
21. Read `packages/tddy-workspace-web/src/models/ProvidersPanel.tsx` (1-40) and `AddProviderForm.tsx` (1-30); grep the RPCs in `models.proto`.
22. Read `packages/tddy-workspace-web/docs/daemon-settings-screen.md` and `docs/ft/daemon/daemon-settings.md` (1-60).
23. `ls tddy-model-registry/src` and grep `api_key|secret|vault|credential`: does the model registry store secrets separately? Yes.
24. `ls tddy-host-service/src` and `docs`, then read `packages/tddy-host-service/docs/host-tooling-probe.md` (1-200): the precedent for probing an external CLI.
25. Read `docs/ft/web/hosts-screen.md`.
26. grep `service|rpc|message` in `host.proto`, then read lines 1-107, 160-240 and 355-440.
27. Read `packages/tddy-hosts-web/docs/hosts-screen.md` (headings, 23-232, 384-443). `wc -l` the host row components, then read `HostsContent.tsx`.
28. Read `HostsScreen.tsx` and `HostRowSshConfig.tsx`.
29. `git status`, `git log -8`, `ls docs/dev/1-WIP/`.
30. Read `packages/tddy-host-service/docs/host-registry.md` (1-200).
31. grep `host-desktops` across docs and packages; `ls packages/tddy-web/docs`.
32. Read `packages/tddy-web/docs/host-directory.md` (1-90), `capability-gating.md` (1-60) and `docs/dev/changesets/2026-10-20-host-desktops-rdp-nla-sign-in.md`.
33. grep `capabilitiesForHint|IPC_CAPABILITIES|ConnectionCapability` and read `tddy-client-web/src/connections/types.ts`: where the capability set is defined.
34. `ls tddy-usage` and grep `binding`: what "usage bindings" are.
35. grep `accounts|AccountAssignment` in `project.proto`, then read lines 20-40, 60-90 and 155-215. Read `docs/ft/daemon/harness-profiles.md` (1-50).
36. Read `docs/ft/daemon/provider-accounts.md` (1-80).
37. grep `coder\.com|github.com/coder/coder|coder ssh|coder list|CODER_URL|CODER_SESSION_TOKEN|coder login` across the repo: any prior Coder work.
38. grep `sign-in|login|credential|token` in `docs/ft/daemon/warp-cli-session.md`; read `docs/ft/daemon/codex-oauth-relay.md` (1-30).
39. grep `write_atomic*` definitions and the `screen-sharing` provider const; read `docs/dev/todo/2026-08-16-the-daemon-s-secret-stores-still-truncate-in-place.md`.
40. `ls docs/dev/todo | wc` (457 entries; 201 match the broad terms). Then a targeted grep, printing each hit's H1, for `accounts? screen|AccountsService|credential vault|keyring|identity|provider account|provider-route|secret store|hosts screen|known.?hosts|host row|host registry|ssh config|local gh|\bcoder\b`.
41. Read the heads of 8 key todo entries (ssh-agent-launch, add-account-ungated, project-assignment-one-host, preset-mirror, warp-provider-account, desktop-install-identity, keyring-sync-single-subject, atomic-file-leaf-crate).
42. Read `packages/tddy-desktop/docs/config-resolution-and-install.md` (150-170), grep `AUTH_STORAGE` in `./install`, read `docs/dev/todo/2026-11-01-install-script-over-length-budget.md`.
43. Read the heads of the todos for local-gh child sessions, `host_id` on `DaemonAdvertisement`, and tool sessions ignoring the provider default.
44. Peeked at the parent's WIP discovery doc (1-40); grep `LocalShell`.
45. grep the `SessionAccountAccess` / `session_identity` seam in `tddy-session-lifecycle`.
46. grep `LocalShell` context in `session.proto`; grep `enum ShellKind` in `tddy-tool-engine/src/shell.rs`.
47. grep the linker and service handlers in `tddy-daemon/src/account_linking.rs` and `tddy-accounts/src/service.rs`.
48. grep `shell-menu-*` test ids and the `*_ROUTE` constants: the nav order.

---

#### 2. Inspected files (why, excerpts, line numbers)

### Accounts service (`tddy-accounts`)

**`packages/tddy-accounts/docs/accounts-service.md`**: the service contract.
- `accounts.AccountsService` is "what a person can see and curate in the credential vault". There is no tonic adapter and nothing on the daemon's Unix socket; it is reached over whatever wire the host connection was opened on.
- `AccountSummary` has no secret field (enforced by message shape). `has_secret` is one bit. `subject` is `metadata["subject"]`.
- `account_id` never changes; `label` is the only mutable field.
- The outcomes are never collapsed: `Ok([])`, `Uninitialized`, `Locked`, `NoSuchSession`, `NotFound`, `Unavailable`.
- Registration: "The daemon registers it only when `auth_storage` gives it credential vaults."
- Dependencies: `tddy-credentials`, `tddy-credential-sync`, `tddy-service`, `tddy-rpc`. Not `tddy-daemon-auth`, not LiveKit.
- `resolve_account(assignments, provider, held)` answers one of `Assigned | NotAssigned | UnknownOnThisHost | Ambiguous`. There is no fallback.
- The optional builders (`with_linking`, `with_provider_accounts`, `with_route_support`, `with_provider_account_defaults`, `with_sync_status`) each answer `FAILED_PRECONDITION` when unwired.

**`packages/tddy-accounts/src/lib.rs`**: module list `attempts, deadline, identity, linking, local_gh (pub), provider_account, resolver, service, store, sync_status, vault_store`. Re-exports include `acting_identity, ActingIdentity, GitIdentity, PROVIDER_GITHUB`, `AccountLinker, LinkChallenge, LinkProgress, LinkedAccountStore, META_SUBJECT, META_SUBJECT_ID`, and `ProviderAccount, ProviderModelProbe, RouteSupport, presets, WireProtocol, META_DEFAULT_FOR`. The doc says: "One provider's authorization dance is implemented behind [`AccountLinker`]; each further provider is its own work."

**`packages/tddy-accounts/src/provider_account.rs`** (795 lines):
- L1-11: a model-provider account "is an ordinary `CredentialRecord` — `provider` is the preset id, `secret` is the key, and the endpoints it serves live in `metadata`."
- L22-29: `enum WireProtocol { AnthropicMessages, OpenAiResponses, OpenAiChat }`.
- L45-52: `enum PresetEndpoint { NotServed, Fixed(&'static str), OperatorSupplied }`.
- L113-160: a static table of 6 presets: `meta, fireworks, baseten, modal, openai-compatible, anthropic-compatible`.
- Per the doc, `ProviderAccount::from_record` is `None` for any non-provider record (GitHub, screen-sharing). Ports: `ProviderModelProbe` (HTTP model listing), `ProviderAccountStore`, `RouteSupport`, `ProviderAccountDefaults`. Metadata keys: `preset`, `probed_at`, `endpoint.<proto>`, `default_for`.

**`packages/tddy-accounts/docs/provider-accounts.md`**: the ports table above. `account_id_for(preset,label)` gives `<preset>-<label slug>`; `unclaimed_account_id` numbers duplicates `-2…`. `put_provider_account` never overwrites (store-wide mutex).

**`packages/tddy-accounts/docs/account-linking.md`** and **`src/linking.rs`** (L1-120):
- `META_SUBJECT_ID = "subject_id"` (immutable provider id; dedup key) and `META_SUBJECT = "subject"` (display login).
- `LinkChallenge { link_id, user_code, verification_uri, expires_in_seconds, interval_seconds }` is device-code shaped.
- `LinkProgress { Pending{interval}, Approved{identity, access_token}, Denied, Expired }`.
- `LinkError { NoSuchSession, Locked, NoSuchLink, UnsupportedProvider(String), Unavailable }`.
- `trait AccountLinker { fn begin(&self, provider:&ProviderId)->Result<LinkChallenge,LinkError>; fn poll(&self, link_id:&str)->Result<LinkProgress,LinkError>; }`
- No match on link creates `github-<subject id>`. Linking never mints a session.

**`packages/tddy-accounts/docs/github-identity-resolution.md`**: `acting_identity(assignments, provider, held)` returns `ActingIdentity { account, token, git }` (the token and the commit identity are inseparable). The token reaches a session "asked for, per call, over a socket the session's host serves — never an environment variable, never a file." Nothing reads `GITHUB_TOKEN`/`GH_TOKEN`. In the Local gh section, `GithubCli` lives in `tddy-host-service` and this crate stays free of subprocesses.

**`packages/tddy-accounts/src/local_gh.rs`**: `github_hostname_of(git_url)` handles the https, scp and ssh:// forms and refuses an unreadable URL. `local_gh_git_identity(login,id)`. The module doc says: "A Local gh project has no vault account: each host's own `gh` login … is the identity."

**`packages/tddy-accounts/src/store.rs`**: `enum AccountsError { NoSuchSession, Locked, Uninitialized, NotFound{provider,account}, Unavailable(String) }` and `trait AccountStore { list(token)->Vec<CredentialRecord>; set_label(...); remove(...) }`. It is expressed over `tddy-credentials` data types, not over `SessionVault`.

**`packages/tddy-accounts/src/service.rs`**:
- L171-175 `require_linking` gives `failed_precondition("this daemon cannot link accounts: no store is wired")`.
- L192-204 `connect_provider_account`: the vault is listed before the network is touched.
- L397-409 `begin_link_account`: `ProviderId::new(request.provider)`, i.e. the provider is an open string.

### Wire: `packages/tddy-service/proto/accounts.proto`
- L21-45 `service AccountsService`: `ListAccounts, SetAccountLabel, RemoveAccount, BeginLinkAccount, PollLinkAccount, ConnectProviderAccount, ListProviderAccountModels, ListProviderRoutes, SetProviderAccountDefaults`.
- L54-86 `message AccountSummary { string provider=1; string account_id=2; string label=3; string subject=4; int64 updated_at=5; bool has_secret=6; SyncStatus sync_status=7; string preset=8; repeated string wire_protocols=9; repeated string default_for=10; }`. L55-56: "Which service this authenticates against — "github", "cloudflare", "screen-sharing". Open rather than an enum: a new provider must not be a breaking change to every reader."
- L107-110 `ProviderAccounts { provider, accounts }`; L126-140 `ListAccountsResponse { providers, vault_locked, vault_uninitialized, optional SessionAccount session_account }`.
- L199-204 `BeginLinkAccountRequest { session_token, provider }` ("github" today).
- L246-256 `ConnectProviderAccountRequest { session_token, preset, label, api_key, anthropic_base_url, openai_base_url }`. `api_key` is inbound only.
- L271-284 `ListProviderRoutesRequest { harness }`, `ProviderRouteOption { account_id, label, preset, supported, reason }`.

### Credential store (`tddy-credentials`)

**`packages/tddy-credentials/docs/credential-store.md`** (332 lines):
- `ProviderId` is "a newtype, not an enum: adding a provider is data". `AccountId` is distinct from the vault owner.
- `CredentialRecord { provider, account, label, secret, metadata, updated_at, version }`. `SecretString` is redacted, zeroized and not serde. There are `Tombstone` and `VaultEntry`.
- File layout: `CredentialStore::path_in(auth_storage, subject)` gives `auth_storage/credentials-<hex subject>.vault`, mode 0600, swap-then-rename.
- KDF: Argon2id passphrase KEK → data key DK; records are ChaCha20-Poly1305 under DK; record ids are an HMAC so no account name is in cleartext; browser unlock slots.
- "There is no daemon-held key." `SessionVaults` states are `Open | Locked | Uninitialized`.

**`packages/tddy-credentials/src/record.rs`** L79-104: the `CredentialRecord` struct. `secret: SecretString` is commented "Never returned to an RPC response path"; `metadata: BTreeMap<String,String>`.

**`packages/tddy-credentials/src/sessions.rs`** API: `SessionVaults::new(auth_storage_dir)`, `with_idle_lifetime`, `state(subject)`, `retain(subject, record)`, `sign_in_without_credential`, `unlock`, `create`, `reset`, `reopen`, `forget`, `get`, `use_open(subject)`.

### Daemon config

**`packages/tddy-daemon-kernel/src/config.rs`**:
- L63-64 `DaemonConfig` is `#[serde(deny_unknown_fields)]`. L69-70 `github: Option<GitHubConfig>`.
- L73-80 `auth_storage: Option<PathBuf>`: "Directory holding server-side auth state: this daemon's Ed25519 session-token signing key (`signing_key.pem`…) and each user's credential vault (`credentials-<hex login>.vault`, mode `0600`)…"
- L88-92 `users: LiveUsers` ("The GitHub-login → OS-user map every token-gated RPC authorizes by"), `skip_serializing`.
- Also `allowed_tools`, `allowed_agents`, `daemon_instance_id`, `screen_sharing`, `git: Option<GitConfig>`, `keyring: Option<KeyringConfig>`, `relay`, `telegram`, `tddy_data_dir`.
- Tests L923-949: a `users:` key in daemon.yaml fails the load, naming `users.yaml`; a missing `users.yaml` also fails the load.

**`packages/tddy-daemon-kernel/src/config/access.rs`** L1-46:
- `GitHubConfig { client_id, client_secret, redirect_uri, stub, stub_codes, pending_login_ttl_seconds, open_vault_idle_ttl_seconds, dev_vault_without_passphrase }`
- `UserMapping { github_user, os_user }`
- `AllowedTool { path, label }`, `AllowedAgent { id, label }`

**`packages/tddy-daemon-kernel/src/config/sections.rs`** L49-60: `KeyringConfig { group_secret: Option<String> }`; absent means sync with nobody. L64-75: `GitConfig { ssh_command }` (overrides `GIT_SSH_COMMAND` for daemon fetches only).

**`packages/tddy-daemon-kernel/src/live_users.rs`** L1-80: `LiveUsers` is an Arc-shared holder of `Vec<UserMapping>` with `owned_by_supervisor` and `enrol_first_login`.

**`packages/tddy-users-file/README.md`**: `UsersMapping::from_yaml`; `MappedUser { github_user, os_user }`; at most 15 OS users; no root.

**YAMLs**:
- `dev.daemon.yaml` L39-42 `auth_storage: tmp/auth` ("Without it no AccountsService is registered"). L78-85 `github: { stub: true, stub_codes: "test-code:testuser", dev_vault_without_passphrase: true }`. L95 "The GitHub → OS-user mapping lives in users.yaml beside this file". L142+ `allowed_agents`.
- `daemon.yaml.production` L30 `auth_storage: "__AUTH_STORAGE_DIR__/auth"`; L158-159 "the daemon refuses a `users:` key here".
- `dev.desktop.yaml` L57 `auth_storage: tmp/.tddy/auth`; L77-85 a stub github block.

### Daemon wiring
- **`packages/tddy-daemon/src/runtime/mount_accounts.rs`**:
- `mount_screen_sharing_entry` builds `ScreenSharingServiceImpl…with_host_scope(FileHostDesktopTargetStore(host_registry_dir), FileHostKeypair, host_prompts).with_credential_vaults(...)`.
- `mount_accounts_entry`: `if let Some(vaults) = self.identity.credential_vaults.clone() { accounts_service_over(&vaults, user_resolver, &auth_config, coding_agents) … push(build_accounts_entry(..)); m.hosted.rpc_handlers.attach_provider_accounts(accounts) }`. The same instance is read by `DescribeSessionStartOptions`.
- `wire_credential_sync` runs only if a common room exists.
- **`packages/tddy-daemon/src/account_linking.rs`**:
- L77-80 `impl AccountLinker for GitHubAccountLinker { fn begin(..) { if provider != github → UnsupportedProvider } }`.
- L262-289 `accounts_service_over(vaults, subject_of, config, coding_agents)` = `AccountsServiceImpl::new(store).with_provider_account_defaults(store).with_provider_accounts(store, HttpProviderModelProbe).with_route_support(HarnessRouteSupport::new(Some(coding_agents)))`, plus `with_linking(GitHubAccountLinker, VaultLinkedAccountStore)` when `github_account_linking_provider(config)` is Some (never for a stub).
- **`packages/tddy-daemon/src/provider_route_support.rs`** L1-50: `HarnessRouteSupport` over `tddy_core::provider_route::{agent_routable, route_supported}`, reading each agent spec's `coding_agent.route` from the host's `CodingAgentRegistry`.

### Web Accounts / Settings
- **`docs/ft/web/accounts-screen.md`**: `#/accounts`; rows are grouped by provider; Rename, Remove, **Add account** (device-code link, shown for every provider group), **Connect API key** (model-provider presets), Models, Default for (claude-cli / opencode-cli / muse-cli). Four outcomes are never collapsed. "A daemon without `auth_storage` keeps no credentials and does not offer the service." Not capability-gated.
- **`packages/tddy-workspace-web/docs/accounts-screen.md`**: route `ACCOUNTS_ROUTE="/accounts"` (`tddy-web/src/routing/appRoutes.ts:71`). `AccountsContent` (data) and `AccountsScreen` (presentational). Test ids `accounts-group-<provider>`, `accounts-row-<provider>-<accountId>`. `PROVIDER_PRESET_OPTIONS` mirrors the daemon table by hand.
- **`packages/tddy-web/src/components/accounts/AccountsAppPage.tsx`** (11 lines): `<AppShell title="Accounts"><AccountsContent/></AppShell>`.
- **`packages/tddy-workspace-web/src/accounts/AccountsScreen.tsx`** L26-77: `AccountRow { accountId, label, subject, updatedAtUnixSeconds, hasSecret, syncStatus, preset, wireProtocols, defaultFor }`, `ProviderGroup { provider, accounts }`, `SessionAccountRef`, `AccountsOutcome = listed|uninitialized|locked|error`.
- Other account UI files: `AccountsContent.tsx` (292), `ConnectProviderAccountForm.tsx` (174), `ProviderAccountDefaultsControl.tsx` (69), `ProviderModels.tsx` (58). `packages/tddy-workspace-web/src/projects/ProjectAccountAssignment.tsx` is the per-project account picker. `ProjectsContent.tsx:161-166` shows the message "Accounts are not enabled on this host — its daemon config sets no auth_storage…".
- **Models screen (a separate credential store)**: `packages/tddy-workspace-web/src/models/ProvidersPanel.tsx` L18-26 and `AddProviderForm.tsx` L5-17 (`PROVIDER_KIND_OPTIONS`: Ollama, OpenAI, Fireworks, Anthropic; the key is inbound only). This is backed by `models.proto` `ModelRegistryService` (`ListProviders/CreateProvider/DeleteProvider/…`, L14-63) and `packages/tddy-model-registry/src/store.rs` `credential_for(provider_id, caller)`. Per `docs/ft/daemon/provider-accounts.md`, the model registry is `models.db`, "a per-daemon, plaintext store", kept separate from the vault.
- **Settings**: `packages/tddy-workspace-web/src/settings/DaemonSettingsScreen.tsx` and `settingsForm.ts`; the route is `SETTINGS_ROUTE="/settings"` (appRoutes.ts:96). `docs/ft/daemon/daemon-settings.md`: `DaemonConfigService` `GetConfig` (secrets redacted), `UpdateConfig` (writes the YAML; the YAML stays the source of truth), `GetClientConfig` (ungated), `GetIntegrationStatus` (LiveKit and Telegram: Disabled, Misconfigured, Connecting, Connected, Unreachable).
- **Nav** (`packages/tddy-web/src/components/shell/DaemonNavMenu.tsx`): `shell-menu-` sessions L67, worktrees L77, tasks L87, projects L97, models L107, hosts L117, accounts L127, vms L137, livekit L147, rpc-playground L157, settings L167.

### Projects ↔ accounts (`packages/tddy-service/proto/project.proto`)
- L26-31 `rpc SetProjectAccounts`: the list replaces the stored one; at most one per provider; logical-project scope.
- L70-71 `OrchestratorDefaults.provider_account_id`.
- L79-84 `message AccountAssignment { string provider = 1; /* "github", "cloudflare", ... */ string account_id = 2; }`
- `ProjectEntry` L164 `repeated AccountAssignment accounts = 8;` and L166-168 `bool local_gh = 9;` (a project has one GitHub source).
- L204-209 `SetProjectAccountsRequest { accounts, daemon_instance_id, local_gh }`.

### Coding-agent providers / routes / harness profiles / usage
- **`docs/ft/daemon/provider-accounts.md`**: the preset table, and the capability matrix from each spec's `coding_agent.route` (`providers`, `shape: env|toml_args|json_config_file|inline_config`, `key_env`, `config_env`). Warp: "never: Warp runs on the person's own Warp sign-in".
- **`docs/ft/daemon/harness-profiles.md`**: `<tddyhome>/agents/*.yaml` with `harness:`, `provider_account:`, `model:`. YAML only, no UI.
- **`packages/tddy-usage/docs/usage-bindings.md`** and `src/*`: "usage bindings" are an agent spec's `usage:` section, which maps transcript and SQLite records to token accounting. They are not credentials or accounts.

### External CLI precedent: host tooling probe
**`packages/tddy-host-service/docs/host-tooling-probe.md`** (L1-200):
- `trait HostToolingProbe { fn probe(&self, os_user:&str, github_hostname:Option<&str>) -> HostTooling; }`
- `HostTooling { git, github_cli, ssh_agent, remote_desktop }`, each with a `ProbeOutcome` (Ok | Failed(reason) | Unsupported).
- `classify_gh_auth_status(exit_code, output)` is a pure function; unrecognised output is a failure, never a negative finding.
- `github_cli.rs` `GithubCli::{logged_in_user(os_user,hostname) → gh api user, auth_token(os_user,hostname) → gh auth token}` with `GithubCliRefusal { NotInstalled, NotLoggedIn{hostname,os_user}, Failed }`. `on_spawn_child_path()` looks `gh` up per call.
- `spawn_as_user` removes `GH_TOKEN, GITHUB_TOKEN, GH_ENTERPRISE_TOKEN, GITHUB_ENTERPRISE_TOKEN`.
- Programs are resolved via `spawner::find_program_on_spawn_child_path`; absence of `gh` is "not installed".
- The probe runs as the OS user (`start_output_as_user`: getpwnam_r, HOME, PATH, setgid/initgroups/setuid). `PROBE_TIMEOUT` is 5 s for the whole probe.
- Tests use `a_gh_cli` stub builders from `tddy-testing-commons`. Source files: `packages/tddy-host-service/src/{host_tooling.rs, host_tooling/, github_cli.rs, ssh_config.rs, ssh_agent.rs, host_registry.rs, host_registry/}`.

### Hosts screen

**`docs/ft/web/hosts-screen.md`**:
- The list is durable and daemon-side. A host is never removed. Online is never stored; it is computed against the live roster.
- Columns: Host (`(local)`), Status, Last seen, Tooling (online rows only), Instance ID, Repos base path. Rows sort online first, then by label.
- The list is read once per visit and re-read on a host change.

**`packages/tddy-service/proto/host.proto`**:
- L13-85 `service HostService`: `ListEligibleDaemons, ListKnownHosts, GetHostTooling, StreamHostPrompts, AnswerHostPrompt, AddHostKey, ListHostKeyCandidates, ListSshConfigHosts, StreamHostStats`. Per-host RPCs are addressed by `daemon_instance_id` and peer-routed.
- L162-166 `EligibleDaemonEntry { instance_id, label, is_local }`.
- L168-175 `GetHostToolingRequest { session_token, daemon_instance_id, github_hostname }`; L177-185 `GetHostToolingResponse { daemon_instance_id, HostGitIdentity git, HostGithubCli github_cli, HostSshAgent ssh_agent, repeated HostRemoteDesktop remote_desktop }`.
- L217-225 `HostGithubCli { ProbeOutcome outcome; bool installed; bool authenticated; string login; string failure_reason }`. Its comment: "This is the HOST's gh login — not the tddy web session's user, and not a GITHUB_TOKEN".
- L359-372 `KnownHostEntry { string instance_id=1; string label=2; bool online=3; int64 first_seen_unix_ms=4; int64 last_seen_unix_ms=5; string repos_base_path=6; uint64 max_attachment_bytes=7; bool is_local=8; }`. **It has no capabilities field.**
- L395-414 `ListSshConfigHostsRequest { session_token, daemon_instance_id }`, `SshConfigHost { alias }`, `ListSshConfigHostsResponse { ProbeOutcome outcome; failure_reason; repeated SshConfigHost hosts }`, with the comment "empty is LocalShell as a choice, FAILED is not a choice."
- L416-422 `ListKnownHostsRequest { session_token }`, `ListKnownHostsResponse { hosts }`. L425-ish `enum ProbeOutcome { UNSPECIFIED, OK, FAILED, UNSUPPORTED }`.

**Hosts web components**:
- `packages/tddy-hosts-web/src/HostsContent.tsx` (70): L16-25 `rowFromRpc(host) → { instanceId, label, online, lastSeenUnixMs, reposBasePath, isLocal }` (drops `firstSeenUnixMs` and `maxAttachmentBytes`). L27-62: one `client.listKnownHosts({sessionToken})` per `[client, sessionToken]`, with a `current` flag.
- `packages/tddy-hosts-web/src/HostsScreen.tsx` (124): L16-31 `interface HostRow { instanceId; label; online; lastSeenUnixMs: bigint; reposBasePath; isLocal }`; L46-51 `byLivenessThenLabel`. Table columns (L66-74): Host, Status, Last seen, **Telemetry** (`HostRowTelemetry`), **Tooling** (`HostRowToolingCell`), Instance ID, Repos base path.
- `packages/tddy-hosts-web/src/HostRowSshConfig.tsx` (119): `useHostClient(HostService, instanceId)` → `listSshConfigHosts({sessionToken, daemonInstanceId})`. States: aliases listed, "No SSH hosts", "Could not check", "Not supported here", "…". Test ids `hosts-row-<id>-ssh-config`, `hosts-row-<id>-ssh-config-host-<alias>`.
- Other row parts: `HostRowToolingCell.tsx` (66), `useHostTooling.ts` (49; resolves `useHostClient(HostService, online ? instanceId : null)` and issues `GetHostTooling` once per mount), `HostRowTooling.tsx` (170), `HostRowSshAgent.tsx`, `HostRowRemoteDesktop.tsx` (200), `HostDesktopOverlay.tsx`, `HostDesktopSignInDialog.tsx`, `HostAddKeyAction.tsx`, `HostPassphraseDialog.tsx`, `encryptForHost.ts`, `hostKeyPinning.ts`, `useHostStats.ts`, `HostStatsFooter.tsx`, `VmsScreen.tsx`, `DefineVmPanel.tsx`.

**`packages/tddy-hosts-web/docs/hosts-screen.md`**:
- L41-44: the `HostRow` shape.
- L46-50: `HostRowTooling` takes `{instanceId, git, githubCli, sshAgent, remoteDesktop}` as props.
- L52-57: a guard-first outcome check ("only `ProbeOutcome.OK` licenses a finding").
- L79-86: `HostRowSshConfig` fetches its own RPC; empty means the LocalShell choice.
- L123-129: the Connect desktop button needs `useHasCapability(useHostConnection(id), "media")`; `IPC_CAPABILITIES` is `{"rpc"}`.
- L222-230: `HostRowToolingCell` / `useHostTooling` (loading, answered, failed).
- L384-440: the Rows table and tooling cell strings (git, gh, ssh-agent, remote desktop).

**`packages/tddy-host-service/docs/host-registry.md`**:
- Storage `<tddy_data_dir>/hosts/known-hosts.json` (`KnownHost { instance_id, label, first_seen_unix_ms, last_seen_unix_ms, repos_base_path, max_attachment_bytes }`), plain `write_atomic`; "A host list is not a credential."
- Host-scoped desktop targets: `host-desktop-targets.json` `{ hosts: { <daemon_instance_id>: [HostDesktopTarget{target_id,label,host,port,protocol,username,display?}] } }`, also not a credential store.
- A desktop credential is remembered only on Remember, as a `screen-sharing` vault record under `host:<daemon_instance_id>:<target_id>`. The prompt answer is encrypted to the host keypair and passed to the bridge on stdin.
- Identity: `local_instance_id_for_config` (routing, may carry a startup suffix) vs `local_base_instance_id_for_config` (durable, used by the registry). `host_id` is published beside `DaemonAdvertisement` via `PublishedDaemonMetadata`. `CommonRoomPeerRegistry::apply_snapshot` gives one `record_snapshot` per room snapshot.

**`packages/tddy-web/docs/host-directory.md`**: `HostDescriptor { hostId, label, sourceId, reposBasePath?, maxAttachmentBytes? }`; sources are `livekit` and `serving`; `mergeHostDirectory` (first source wins; optimistic status).

**`packages/tddy-web/docs/capability-gating.md`** and **`packages/tddy-client-web/src/connections/types.ts`**: L33 `export type ConnectionCapability = "rpc" | "media" | "presence";`, L60 `interface HostConnection { readonly hostId … }`, L90 `readonly capabilities: ReadonlySet<ConnectionCapability>`. `useHasCapability` is the one predicate; `capabilitiesForHint` is in `sessionAttachment.ts:69`.

**`docs/dev/changesets/2026-10-20-host-desktops-rdp-nla-sign-in.md`**: `#host-desktops` 2/8 (#654): `Credentials { username, domain, password }` in `tddy-screenshare`; RDP CredSSP/NLA.

### Remote shell (existing, adjacent)
- `packages/tddy-service/proto/session.proto` L372-375 `string ssh_config_host = 33;` (session entry) and L708-714 `string ssh_config_host = 38;` (start request): "OpenSSH `Host` alias … Empty = LocalShell … every exec-catalog tool runs there via RemoteShell (`ssh -o BatchMode=yes <alias>`) … never a fallback to LocalShell."
- `packages/tddy-tool-engine/src/shell.rs:38` `pub enum ShellKind { Local, Remote }`, L45 `trait Shell`; test `packages/tddy-tool-engine/tests/remote_shell_exec_acceptance.rs`.

### Other auth/identity notes
- `docs/ft/daemon/warp-cli-session.md` L16, L62-63: Warp takes no provider account; the CLI carries its own browser/device-code login on first run.
- `docs/ft/daemon/codex-oauth-relay.md`: Codex CLI OAuth is relayed through a loopback TCP tunnel over LiveKit (`LoopbackTunnelService.StreamBytes`). This is the precedent for an external CLI's own browser login being bridged to the operator.
- `packages/tddy-session-lifecycle/src/connection_service/svc_session_identity_wiring.rs:48` and `svc_agent_host_ports.rs:129` `session_identity(os_user, session_id, project_id, session_token) -> SessionIdentity`: where account identity is applied at session start.
- `packages/tddy-session-store/src/atomic_file.rs` L29 `write_atomic`, L44 `write_atomic_with_mode`, L75 `write_atomic_labelled`; `tddy-credentials/src/atomic.rs` `write_owner_only` (duplicated).
- `packages/tddy-screen-sharing/src/screen_sharing_records.rs:31` `pub const SCREEN_SHARING_PROVIDER: &str = "screen-sharing";`, L48 `ProviderId::new(SCREEN_SHARING_PROVIDER)`. This is the existing example of a **non-GitHub, non-model-provider vault provider** owned by its own crate.
- Install: `./install` L94 `INSTALL_AUTH_STORAGE_DIR` ("OAuth/token storage base (default: /var/lib/tddy; --user: …)"), L254/268 `AUTH_STORAGE_DIR` defaults, L281 `AUTH_STORAGE_PATH="${AUTH_STORAGE_DIR}/auth"`, L363-366 desktop variant, L1198/L1461 `chmod 0700`. `packages/tddy-desktop/docs/config-resolution-and-install.md` L163 lists `INSTALL_AUTH_STORAGE_DIR` among the path overrides.

---

#### 3. Grep / glob patterns and notable hits

| Pattern | Scope | Notable hits |
|---|---|---|
| `auth_storage\|INSTALL_AUTH_STORAGE_DIR` | packages, docs, scripts | `tddy-daemon-kernel/src/config.rs:80`; `tddy-daemon-auth/src/auth.rs:37,153,189,545-548`; `tddy-workspace-web/src/projects/ProjectsContent.tsx:161-166`; `tddy-desktop/src-tauri/src/config_source.rs:362,392`; `tddy-vm/src/tddy_host.rs:274`; `tddy-daemon-auth/tests/auth_storage_posture_warning_acceptance.rs` |
| find in 7 account and host packages | packages | see §2. Notable docs: `tddy-daemon-auth/docs/changesets/2026-11-09-user-mapping-in-users-yaml.md`, `tddy-accounts/docs/changesets/2026-10-21-provider-route-*.md`, `2026-10-15-local-gh-github-source.md` |
| `build_accounts_entry\|accounts_service_over\|with_linking\|with_route_support\|HarnessRouteSupport\|HttpProviderModelProbe` | non-test rs | `tddy-daemon/src/account_linking.rs:19,262,277-283`, `provider_model_probe.rs:15-34`, `provider_route_support.rs:14-38`, `runtime/mount_accounts.rs:62,76` |
| `struct GitHubConfig\|KeyringConfig\|ScreenSharingConfig` | kernel src | `config/access.rs:3`, `config/sections.rs:49`, `config/screen_sharing.rs:6` |
| `^(users\|github\|auth_storage\|keyring\|…):` | root YAMLs | `dev.daemon.yaml:42,78`, `daemon.yaml.production:30,158-159`, `dev.desktop.yaml:57,77` |
| `users.yaml` | kernel src | `config.rs:845-949` (tests refusing `users:` in daemon.yaml or a missing users.yaml) |
| filenames `*account*\|*setting*\|*provider*\|*identity*` | web packages | `tddy-workspace-web/src/accounts/*`, `src/settings/*`, `src/models/ProvidersPanel.tsx`, `AddProviderForm.tsx`, `src/projects/ProjectAccountAssignment.tsx`, `tddy-session-create-web/src/ProjectSettingsPane.tsx`, `tddy-web/src/components/{accounts,settings}/*AppPage.tsx`, cypress `AccountsScreenAcceptance`, `AccountLinkingAcceptance`, `ProviderAccountConnectAcceptance`, `ProjectAccountsAcceptance`, `ProviderAccountDefaultsAcceptance`, `DaemonSettings*` |
| `rpc ` | `models.proto` | `ModelRegistryService` L14-63 (providers with their own stored credential) |
| `api_key\|secret\|vault\|credential` | `tddy-model-registry/src` | `assistant_def.rs:21,100-111` `credential_for`; `acp_service.rs:302-438`; `error.rs:118` "the credential belong[s] to the row's owner" |
| `^service\|rpc\|^message` | `host.proto` | L13-84 RPCs; messages L108-459 |
| `host-desktops` | docs, packages md | `docs/dev/changesets/2026-10-20-host-desktops-rdp-nla-sign-in.md` plus about 19 todo entries (rdp, x11, desktop sign-in, screen-share) |
| `capabilitiesForHint\|IPC_CAPABILITIES\|ConnectionCapability` | tddy-client-web | `connections/types.ts:33`, `sessionAttachment.ts:69`, `capability/capabilityAvailability.ts:56` |
| `binding` | tddy-usage | usage bindings are the spec `usage:` section (`context_file.rs`, `gather.rs`, `counter_expr.rs`). Not accounts |
| `accounts\|AccountAssignment` | `project.proto` | L26-31, 70-71, 79-84, 164-168, 204-209 |
| `coder\.com\|github.com/coder/coder\|coder ssh\|coder list\|CODER_URL\|CODER_SESSION_TOKEN\|coder login` | whole repo | **Only** the parent's WIP doc L22 (`coder ssh --remote-forward`). No Coder integration exists anywhere |
| `write_atomic*` defs, `"screen-sharing"` | packages | `tddy-session-store/src/atomic_file.rs:29,44,75`; `tddy-screen-sharing/src/screen_sharing_records.rs:31,48` |
| todo grep (account, identity, keyring, provider, hosts, ssh config, local gh, coder) | `docs/dev/todo` | about 130 hits; relevant ones in §4.5 |
| `AUTH_STORAGE` | `./install` | L94,140,254,268,276-281,363-366,585,622,1186-1217,1445-1477 |
| `LocalShell` | packages | `session.proto` (via gen) L373,708-712; `tddy-web/cypress/component/SessionsSshExecAcceptance.cy.tsx:3`; `cypress/support/testIds.ts:414`; `tddy-tool-engine/tests/remote_shell_exec_acceptance.rs` |
| `session_identity\|SessionAccountAccess` | tddy-session-lifecycle | `svc_session_identity_wiring.rs:48`, `svc_agent_host_ports.rs:129` |
| `fn begin\|ProviderId::new\|UnsupportedProvider` | `account_linking.rs`, `service.rs` | `account_linking.rs:77-80,239`; `service.rs:171-175,397-409` |
| `shell-menu-*`, `*_ROUTE =` | tddy-web | `DaemonNavMenu.tsx:67-167`; `appRoutes.ts:40-96` (`/accounts` L71, `/hosts` L82, `/settings` L96) |

---

#### 4. Findings

### 4.1 How Accounts are modelled today
- **There is one identity, and it is a GitHub login.** Sign-in is GitHub OAuth (redirect or device flow; the `github:` block in daemon.yaml with `client_id`, optional `client_secret`, `stub`, vault TTLs, `dev_vault_without_passphrase`). The session token resolves to a GitHub login, which is (a) the subject whose vault is opened and (b) mapped to an OS user via `users.yaml` (`UserMapping { github_user, os_user }`; a `users:` key in daemon.yaml is refused). Every token-gated RPC authorizes through `LiveUsers::os_user_for_github`.
- **"Accounts" means records in the signed-in person's credential vault.** A record is `CredentialRecord { provider: ProviderId (open string), account: AccountId, label, secret: SecretString, metadata: BTreeMap<String,String>, updated_at, version }`. Three families exist today:
1. `github`: accounts added by login or by the device-code link. Metadata `subject` (login) and `subject_id` (GitHub id). Ids are `github-<id>` or the login.
2. Model-provider presets (`meta`, `fireworks`, `baseten`, `modal`, `openai-compatible`, `anthropic-compatible`): added by `ConnectProviderAccount` (API key plus an optional base URL, probed by listing models). Metadata `preset`, `endpoint.<proto>`, `probed_at`, `default_for`.
3. `screen-sharing`: desktop credentials (session targets, and host desktops under `host:<daemon_instance_id>:<target_id>` when Remember is set). Owned by `tddy-screen-sharing`, not by `tddy-accounts`.
- **Wire**: `accounts.AccountsService` (`accounts.proto`). `AccountSummary` deliberately has no secret field. `provider` is an open string "so a new provider must not be a breaking change", and the comments already anticipate `"cloudflare"`. `ListAccounts` groups by provider. `BeginLinkAccount.provider` is an open string, but the only `AccountLinker` (`GitHubAccountLinker`) refuses anything not `github` with `UnsupportedProvider`.
- **Projects reference accounts** via `AccountAssignment { provider, account_id }` (`ProjectEntry.accounts`, at most one per provider, no fallback when unassigned). `local_gh: bool` is the alternative "use the host's own CLI login" source for GitHub. Resolution goes through `resolve_account` / `acting_identity`, and the token is delivered per call over the session host socket, never as an env var or a file.
- **Harness/provider routing**: `ListProviderRoutes` / `RouteSupport` over each coding-agent spec's `coding_agent.route` section (`shape: env | toml_args | json_config_file | inline_config`, `key_env`, `config_env`). Defaults per harness come from `default_for` metadata. Harness profiles in `<tddyhome>/agents/*.yaml` name a `provider_account`. "Usage bindings" are an unrelated token-accounting concept.
- **The registry is not pluggable.** Adding a provider kind is done by hand at several points: the preset table in `provider_account.rs` (model providers only, tied to `WireProtocol` and `RouteSupport`), a separate crate owning its own record codec (as `screen-sharing` does), a new `AccountLinker` implementation, and the web's hand-mirrored `PROVIDER_PRESET_OPTIONS`.

### 4.2 How credentials are stored at rest
- **Vault**: one file per subject, `auth_storage/credentials-<hex subject>.vault`, mode 0600, swap-then-rename (`tddy-credentials/src/atomic.rs::write_owner_only`). Argon2id passphrase KEK wraps a random data key; records are ChaCha20-Poly1305 with HMAC record ids, so no account name is in cleartext. Browser unlock slots exist. **There is no daemon-held key**, so the daemon can read a secret only while that person's vault is `Open` in `SessionVaults` (with an idle TTL).
- `auth_storage` (daemon.yaml) also holds `signing_key.pem`. Without it, no `AccountsService` is registered (the web says "Accounts are not enabled on this host"). The install path is `INSTALL_AUTH_STORAGE_DIR` → `<dir>/auth`, chmod 0700 (`./install` L254-281, L363-366). Dev configs use `tmp/auth` and `tmp/.tddy/auth` with `dev_vault_without_passphrase: true` (effectively plaintext, and stub-only).
- **Propagation**: vault records sync between daemons only when `keyring.group_secret` is set and a common room exists. Today it works only while exactly one subject has an open vault (todo `2026-10-05-keyring-sync-single-subject-only.md`).
- **Other stores**:
- The model registry `models.db` is per-daemon, plaintext, and keeps its own provider keys (`credential_for(provider_id, caller)`); it is separate from the vault.
- `known-hosts.json` and `host-desktop-targets.json` under `<tddy_data_dir>/hosts/` are explicitly "not a credential". The host keypair lives in the same directory.
- Daemon config secrets (livekit `api_secret`, `github.client_secret`, telegram token, `keyring.group_secret`) live in the YAML, and `GetConfig` redacts them.
- **External CLIs' own logins** (`gh`, `warp`, `codex login`) are left in the OS user's home (`~/.config/gh/hosts.yml` and so on). tddy only probes them as that OS user (`HostToolingProbe` / `GithubCli`, which strips `GH_TOKEN`/`GITHUB_TOKEN` from the env) or relays their OAuth callback (Codex loopback tunnel). **No per-account external CLI binary path or config exists anywhere.** The `coder` grep found nothing outside the parent's WIP doc.

### 4.3 Where a "Coder account" (CLI + URL + token) could plug in (observations only; the parent draws conclusions)
- **Vault record shape fits without a schema change.** `provider = "coder"`, `account = e.g. coder-<label slug>`, `secret = session token`, `metadata = { url (deployment URL), subject (Coder username), subject_id (Coder user id) }`. `ListAccounts`, `SetAccountLabel`, `RemoveAccount`, credential sync and `AccountSummary { provider, subject, has_secret }` all work for an arbitrary provider string. `ProviderAccount::from_record` returns `None` for non-preset records, so `preset`, `wire_protocols` and `default_for` stay empty.
- **The intake RPC does not fit as-is.**
- `BeginLinkAccount`/`PollLinkAccount` is device-code shaped (`user_code`, `verification_uri`, polling) and dedups on `subject_id`; only `GitHubAccountLinker` exists.
- `ConnectProviderAccount` is "URL + key, probed" in shape, but is bound to the model-provider preset table (`WireProtocol`, an HTTP `/models` probe, `RouteSupport`). Adding `coder` as a 7th preset would put it into the harness route matrix, where it does not belong.
- The nearest precedent for a self-contained provider is `tddy-screen-sharing`, which owns `SCREEN_SHARING_PROVIDER` and its record codec in its own crate. The Warp todo (`2026-10-24-warp-provider-account.md`) is a recorded example of "a preset needs a probe that fits it."
- **The alternative "Local" model is already precedented.** Local gh is the host's own CLI login with no vault record: `GithubCli` runs `gh api user` / `gh auth token` as the OS user, and `HostGithubCli { outcome, installed, authenticated, login }` appears in `GetHostTooling`. A `coder` equivalent would be a `HostToolingProbe` block (installed / authenticated / URL / username via `coder whoami`-like calls) plus a refusal enum mirroring `GithubCliRefusal`.
- **`coder list` maps onto the existing "destinations" listing.** `ListSshConfigHosts` (per host, routed by `daemon_instance_id`, OK-empty vs FAILED kept apart) returns explicit OpenSSH aliases, which feed `ssh_config_host` on `StartSession` (empty means LocalShell, otherwise RemoteShell `ssh -o BatchMode=yes <alias>`). A Coder workspace listing would be the sibling of this RPC and this row cell (`HostRowSshConfig`).
- **Token delivery precedent**: the provider route `(env, …)` shape puts a key in the child env (`key_env`). GitHub tokens are deliberately never env vars (per-call socket). The real `coder` CLI reads `CODER_URL` / `CODER_SESSION_TOKEN` env vars; that is external knowledge, not found in the repo. `spawn_as_user` already scrubs the GitHub token env vars from probes.
- **UI touch points**:
- The Accounts screen (`AccountsScreen.tsx` / `AccountsContent.tsx`, groups keyed by `provider`, "Add account" shown for every group; todo says it is ungated) and a new connect form beside `ConnectProviderAccountForm`.
- Projects' `ProjectAccountAssignment.tsx` / `AccountAssignment { provider: "coder" }`.
- A new Hosts row cell beside `HostRowSshConfig` / `HostRowTooling`.

### 4.4 How the Hosts list is produced and rendered
- **Daemon**: `HostService.ListKnownHosts(session_token)` reads `FileHostRegistry` (`<tddy_data_dir>/hosts/known-hosts.json`), which is fed by `CommonRoomPeerRegistry::apply_snapshot` (one write per room snapshot) plus self-registration at startup. It intersects that with the live roster (`EligibleDaemonSource::live_known_hosts()`, keyed by the durable id `local_base_instance_id_for_config` / the peer's published `host_id`). Rows are never deleted; online is computed per call.
- **What a row knows** (`KnownHostEntry`): `instance_id` (durable daemon id, which per-host RPCs are keyed on), `label`, `online`, `first_seen_unix_ms`, `last_seen_unix_ms`, `repos_base_path`, `max_attachment_bytes`, `is_local`. There are **no capabilities, OS user, platform or tooling fields** on the entry. The web `HostRow` maps only `{instanceId, label, online, lastSeenUnixMs, reposBasePath, isLocal}`.
- **Per-row enrichment is fetched per cell, per host, routed by `daemon_instance_id`.** All of these use `useHostClient(HostService, instanceId)` and run only for online rows (tooling resolves no client when offline):
- `HostRowTelemetry` (stats)
- `HostRowToolingCell` → `useHostTooling` → `GetHostTooling` → `HostRowTooling` (git, gh, ssh-agent, remote desktop plus Connect, and the add-key action)
- `HostRowSshConfig` → `ListSshConfigHosts`
- **Capabilities are a web-side, per-connection concept**: `HostConnection.capabilities: ReadonlySet<"rpc"|"media"|"presence">`, read only through `useHasCapability`. Example: the Connect desktop button requires `"media"`, and `IPC_CAPABILITIES = {"rpc"}`.
- **Host directory** (the selector): a separate, live-only merge of the `livekit` and `serving` sources (`HostDescriptor`). The Hosts screen deliberately does not use it.
- **Host-desktops stack (`#host-desktops`)**: host-scoped desktop targets plus `ScreenSharingService.{ListHostTargets, AddHostTarget, StartHostStream, StopHostStream, InspectHostDesktop}`. Credentials are prompted through `StreamHostPrompts`/`AnswerHostPrompt`, encrypted to the host keypair, and remembered as `screen-sharing` vault records only on Remember.
- **Nav order** (DaemonNavMenu): sessions, worktrees, tasks, projects, models, hosts, accounts, vms, livekit, rpc-playground, settings.

### 4.5 Relevant backlog entries (`docs/dev/todo/`)

Accounts and credentials:
- `2026-10-07-keyring-link-github-add-account-ungated.md`: Add account is shown on daemons that cannot link. Closing it needs a capability field on `ListAccountsResponse`.
- `2026-10-21-web-provider-preset-options-mirror-the-daemon-table.md`: `PROVIDER_PRESET_OPTIONS` is mirrored by hand. Proposes a `ListProviderPresets` RPC.
- `2026-10-24-warp-provider-account.md`: Warp runs only on its own CLI login. A `warp` preset needs a probe that fits a non-HTTP-model key. This is the closest analogue to a CLI-tool account.
- `2026-10-14-project-account-assignment-reaches-one-host.md`: `SetProjectAccounts` is written only to `daemon_instance_id` (`coordinate_handlers.rs:487-528`; the web sends to `group.hosts[0]`).
- `2026-10-04-keyring-accounts-rename-lost-update.md`: `set_label` read-then-write race. The same race applies to defaults.
- `2026-10-04-keyring-accounts-screen-long-components.md`: three `/accounts` components are over 60 lines.
- `2026-10-21-tddy-accounts-service-rs-over-file-budget.md`: `service.rs` is past 500 lines (772 now).
- `2026-10-05-keyring-sync-single-subject-only.md`: sync runs only while exactly one vault is open; the wire carries no subject.
- `2026-09-23-atomic-file-leaf-crate.md`: two copies of the owner-only atomic writer.
- `2026-08-16-the-daemon-s-secret-stores-still-truncate-in-place.md`: closed. Use `write_atomic_with_mode` for any new secret file.
- `2026-10-24-github-account-missing-subject-id-no-commit-identity.md`: a GitHub account with no `subject_id` gives a session no commit identity.
- `2026-10-22-tool-session-starts-ignore-the-provider-default.md`, `2026-10-22-telegram-tool-sessions-choose-no-provider-account.md`, `2026-10-24-harness-profile-empty-account-not-vendor.md`: provider-route gaps.
- `2026-10-15-local-gh-child-sessions-and-telegram-gh.md`: children ignore the parent's forwarded GitHub source; Telegram builds its own `GithubCli`. Relevant if a "local coder" source mirrors Local gh.
- `2026-10-15-local-gh-oversized-files-deferred.md`: files that grew past budget in #622.
- `2026-09-18-desktop-install-configures-no-identity.md`: verification of a fresh desktop sign-in.
- `2026-11-01-install-script-over-length-budget.md`: `./install` is about 2x over budget; the proposed seam is to "split the account/identity handling out".
- `2026-09-23-a-learned-peer-signing-key-is-never-revoked.md`, `2026-08-13-tokengenerator-generate-for-performs-no-authorization.md`: identity and auth debt.

Hosts:
- `2026-09-06-host-id-should-become-a-field-of-daemonadvertisement.md`: `host_id` rides beside the advertisement via `#[serde(flatten)]`.
- `2026-09-06-hosts-row-memory-cell-untested.md`.
- `2026-09-06-ssh-agent-bounds-stop-at-the-conversation.md`.
- `2026-09-27-desktop-mints-a-new-daemon-identity-per-restart-and-orphans-its-sessions.md`.
- The `#host-desktops` set: `2026-10-21-host-prompt-offered-auth-methods-is-never-sent.md`, `2026-10-21-desktop-sign-in-pins-the-advertised-fingerprint.md`, `2026-10-20-endpoint-channel-targets-matched-by-loopback-port.md`, `2026-10-20-x11-display-served-on-its-vnc-port-is-not-offered.md`, `2026-10-20-x11-socket-dir-read-failure-lists-no-channels.md`, `2026-10-20-session-start-without-a-bridge-succeeds.md`, `2026-10-20-rdp-*`, `2026-10-18-screen-share-mid-session-resize.md`, `2026-10-21-desktop-start-helpers-parameter-count-and-nesting.md`.

Remote shell (directly adjacent to `coder ssh`):
- `2026-11-07-ssh-agent-launch-remote-pointer-owner.md`: "No ssh agent launch exists today: `launch_host_pty` runs the agent in a local PTY." `record_remote_owner` / `REMOTE_AGENT_PID_FILE` in `packages/tddy-agent-launch/src/cli_session_pointer.rs` are implemented, have no caller, and carry `#[allow(dead_code)]`.
- `2026-10-29-spawner-read-scope-is-same-host-only.md`: a spawner reads only children on its own host.

There is no backlog entry mentioning Coder, `coder ssh` or `coder list`.

## Exploration 3: split sessions, SSH, remote shell and PTY seams — 2026-11-15

**Agent**: Explore subagent
**Scope**: tddy-session-split, tool-engine Shell, ssh_config_host, agent launch and hooks, PTY

### Discovery dump: split sessions, SSH, and the remote shell/PTY abstractions

Repo: `/Users/mantasi/Code/tddy-coder/.worktrees/plan-remote-shell-transports`. Read-only pass, nothing modified. The only untracked file is your own `docs/dev/1-WIP/2026-11-15-remote-shell-transports-whole-work-initial-discovery.md`.

**The headline:** a "split session" does not run over SSH. It pairs two tddy-daemons over a LiveKit room: the agent runs on daemon A and the git worktree lives on daemon B. SSH is a separate axis, `ssh_config_host`. When it is set, the daemon that manages the code shells out to OpenSSH for worktree setup, tool exec, slot files, the session pointer and git packs. Even then, the coding agent itself still runs in a **local** PTY. Nothing anywhere launches an agent over `ssh -tt`.

---

#### 1. Sequence

1. `ls packages/`, `ls docs/ docs/ft docs/dev/todo`, `git log --oneline -15`: map the workspace and confirm #905–#907.
2. `find` over `tddy-session-split`, `tddy-pty`, `tddy-cli-sessions`, `tddy-askpass`, `tddy-remote-git-repo(-cli)`, plus `wc -l` of their `.rs` files: inventory the crates you named.
3. grep `RemoteShell|remote_shell|PtyBackend|trait \w*Shell` over `*.rs/*.ts/*.tsx/*.md/*.proto`, and list the todo/ft/WIP entries mentioning ssh/split/pty/shell/remote: does any RemoteShell abstraction exist?
4. Read `packages/tddy-tool-engine/src/shell.rs` in full, plus the head of the WIP whole-work doc: the existing `Shell` / `LocalShell` / `RemoteShell`.
5. Read `tddy-session-split` `src/lib.rs`, `docs/module-layout.md` and `Cargo.toml`: crate structure and dependencies.
6. grep `ssh|GIT_SSH|askpass` in `tddy-session-split/src`, and list the `pub` items of `split_session.rs`: how SSH touches split.
7. Read `split_session.rs` lines 1–470: split wiring, `RemoteToolEnv`, `ssh_client_daemon_instance_id`.
8. Read `docs/ft/daemon/remote-managed-worktree.md` in full (from the persisted tool-result file): the PRD for split and SSH.
9. grep `ssh_config_host|setup_worktree_for_session_over_ssh|run_ssh_batch|ListSshConfigHosts|list_ssh_config_hosts|session_shell(` across `packages`: everything that consumes the SSH alias.
10. Read `tddy-git/src/ssh_exec.rs`, `tddy-git/src/ssh_worktree.rs`, and grep the callers of `execute_tool_on_shell` / `session_shell`: the OpenSSH primitives and who uses them.
11. Read `tddy-agent-launch/src/opencode_cli_spawn.rs` lines 100–420: the generic CLI harness start with an ssh alias.
12. grep `PointerHost|write_slot_files_on_target|remote_host|"ssh"|-tt|-t` in agent-launch, cli-sessions, pty and core: is any agent launched over ssh?
13. Read `cli_session_pointer.rs` lines 1–320 and `opencode_cli_spawn/harness_launch.rs`: `PointerHost`, and how the harness is launched.
14. Read `docs/dev/todo/2026-11-07-ssh-agent-launch-remote-pointer-owner.md`. A chained `echo ====` failed, so I re-ran:
15. Read `slot_file_over_ssh.rs` (lines 1–200), `cli_session_worktree.rs` (lines 1–60 and 150–200), and `cli_agent_steps.rs` lines 95–140: ssh options, remote worktree, slot-file writes.
16. Read `svc_spawn_split_agent.rs` in full: the split agent spawn.
17. Read `svc_spawn_split_agent/launch.rs` and `split_agent_pty.rs`: the split agent's argv, hooks and PTY.
18. Read `svc_spawn_split_agent/startup.rs` lines 1–120, plus the crate changesets `2026-11-15-split-agent-liveness.md` and `-hooks.md`: the #905/#906 behaviour.
19. List `docs/dev/changesets` matching split/ssh/pty/shell/remote/transport; read the root `2026-11-15-split-agent-liveness.md` and `-hooks.md`.
20. Read the root changesets `2026-09-14-split-ssh.md`, `-ssh-exec.md`, `-ssh-config-hosts.md` and `2026-11-15-split-session-targets.md`, plus todo `2026-11-15-split-agent-exit-cause-unknown.md`: the SSH stack history and #907.
21. Read `tddy-pty/src/lib.rs`, its `Cargo.toml`, and the `pub` items of `runtime.rs` and `registry.rs`: the PTY abstraction.
22. Read `tddy-pty/src/runtime.rs` lines 20–140 and 270–440: the spawn spec and the `portable_pty` usage.
23. Read `cli_session_manager.rs` lines 1–170 and list every `pub fn` under `cli_session_manager/*`: the CliSessionManager API.
24. Read `cli_session_manager/launch.rs` and `pty_spawn.rs` lines 1–140 and 330–438, plus `pty_handle.rs` lines 1–60: `spawn_rendered_main`, `PtySpawnSpec`, `PtyHandle`.
25. Read the heads and `pub` items of `tddy-terminal-rpc/src/{lib,bridge,local_pty_relay,local_terminal,pty_relay,pty_runtime,session}.rs`: transport-agnostic terminal layers.
26. Read `tddy-terminal-rpc/src/session.rs` lines 45–160 and `pty_runtime.rs` lines 40–120: the `TerminalSession` trait and the daemon-side spawn spec.
27. Read `tddy-remote-git-repo` README, `lib.rs` and the `pub` items of its 4 modules: the `GIT_SSH_COMMAND` shim.
28. Read `tddy-askpass` README and `lib.rs`; grep `GIT_SSH_COMMAND|SSH_ASKPASS|tddy-askpass`: askpass usage.
29. Read `tddy-agent-launch/src/clone_ssh_env.rs` lines 1–130 and `clone_planner.rs` lines 1–80: the clone ssh env.
30. Read `tddy-session-lifecycle/src/connection_service/local_exec_tools.rs` lines 120–200; grep `CloneSshEnv` and `AgentSocketResolver`: the exec routing, and the ssh-agent socket.
31. `ls tddy-host-service/src` and docs; grep `AgentSocketResolver`: where the ssh-agent probe lives.
32. Read `tddy-host-service/src/ssh_agent.rs` lines 1–40, `ssh_config.rs` lines 1–80, and `host.proto` lines 390–420: the ssh-agent probe, alias listing, proto.
33. grep the split and ssh fields in `tddy-service/proto/session.proto`; read lines 365–380 and 700–720: proto representation.
34. grep the core `SessionMetadata` fields and the web `sshConfigHost|codebaseDaemonInstanceId|sshConfigListDaemonId`: where the metadata and web pieces live.
35. Read `CreateSessionSshConfigSelect.tsx` lines 1–60; grep `useCreateSessionPlacement.ts`, `SessionDrawerGroups.tsx`, `sessionRosterHalf.ts`: web representation.
36. Locate `pub struct SessionMetadata`; read `tddy-changeset/src/session_metadata.rs` lines 80–125 and 220–250.
37. `wc` the WIP doc; read `2026-09-14-split-ssh-initial-discovery.md` lines 40–120: prior LocalShell/RemoteShell discovery (makers-lt analogue).
38. Read `cli_branch_starts.rs` lines 215–240; grep ssh in `spec_launch.rs`; read `2026-09-14-remote-git-ssh.md`; grep `spawn_pack_verb`: whether claude-cli honours ssh, and the remote-git over SSH.
39. Read `remote_git_service.rs` lines 640–720 and `ssh_batch_mode_argv`: `PackExecution::Remote`.
40. grep every `Command::new("ssh")|ssh_batch_command|ssh_batch_mode_argv|run_ssh_batch|run_over_ssh(` in non-test code: full inventory of ssh spawn sites.
41. Read `tddy-daemon-extensions` split_rooms / terminal_exposure traits and the `extension-ports.md` split-rooms section; grep `enum SessionToolTransport`: the LiveKit ports.
42. `ls docs/ft/web/1-WIP`; grep docs for `ssh_config_host|RemoteShell|split session|split placement`; read the heads of six ssh/split todos.
43. grep SSH in `terminal-sessions.md`, `claude-cli-session.md`, `cursor-cli-session.md`, `session-room.md`; list agent-launch files using ssh aliases.
44. Read `claude-cli-session.md` lines 535–590 and grep ssh in `svc_start_sandboxed_cli_harness_session.rs` / `project_home_start_spec.rs`: the documented "ssh checkout" contract.
45. Read `spec_launch.rs` lines 290–335 (`launch_host_pty`): confirm the agent spawn is a local PTY.
46. Read the full WIP doc (26 lines) and `git status`: the recorded user constraint.
47. grep `HookCommandParams`; read `tddy-agent-backend/src/claude_hooks.rs` lines 1–140: the hook command shape (relevant to that constraint).

---

#### 2. Inspected files (paths, why, load-bearing excerpts)

### `packages/tddy-tool-engine/src/shell.rs`: the existing `Shell` / `LocalShell` / `RemoteShell`
- Module doc, lines 1–6: "The coding agent never opens SSH from its jail. Dispatch stays IPC/HTTP/LiveKit to the code-managing daemon; that daemon picks `LocalShell` (empty `ssh_config_host`) or `RemoteShell` (`ssh -o BatchMode=yes <alias>`). A BatchMode failure is the result — never a fallback."
- The trait, lines 44–52:
```rust
#[async_trait]
pub trait Shell: Send + Sync {
    fn kind(&self) -> ShellKind;          // Local | Remote
    fn root(&self) -> &str;
    async fn read_to_string(&self, path: &str) -> Result<String, ShellError>;
    async fn write_string(&self, path: &str, contents: &str) -> Result<(), ShellError>;
    async fn run(&self, command: &str) -> Result<std::process::Output, ShellError>;
}
```
- `ShellError` has two variants, `Unimplemented(String)` and `Io(String)` (lines 19–24). `ShellKind` is `{Local, Remote}` (lines 38–41).
- `LocalShell { root: PathBuf }`, lines 55–94: `contain_path` plus `std::fs`, and `run` goes through `run_contained(command, &self.root, &[], None)`.
- `RemoteShell { host_alias, remote_root }`, lines 98–181:
- `ssh_argv()` builds `["ssh","-o","BatchMode=yes","--",alias,cmd]`.
- `ssh_run` calls `tokio::task::spawn_blocking(run_ssh_batch)`.
- read is `cat '<abs>'`.
- write is `set -e; mkdir -p …; printf %s '<base64>' | base64 -d > …`.
- run is `cd '<root>' && <command>`.
- Containment uses `contain_remote_path`.
- `session_shell(local_root, ssh_config_host) -> Box<dyn Shell>`, lines 188–199. An empty alias gives Local; otherwise Remote, with the root as a lossy string.
- `execute_tool_on_shell`, lines 202–225. `ShellKind::Local` calls `execute_tool_with_env`. `ShellKind::Remote` calls `execute_tool_on_remote_shell`, lines 227–273, which:
- re-implements Read, Write, StrReplace, Delete, Grep (`rg --json`), Glob (`find`), Shell and Await;
- returns explicit "not available over RemoteShell" errors for ReadLints, the Lsp* tools, SemanticSearch and restructure tools;
- refuses `Shell` with `block_until_ms == 0`: "background jobs are not supported over RemoteShell" (line 437).
- **Note:** dispatch switches on `kind()`, not on trait polymorphism. The remote tools are free functions over `&dyn Shell`.

### `packages/tddy-git/src/ssh_exec.rs`: the OpenSSH primitives, re-exported via `tddy_core` (`packages/tddy-core/src/lib.rs:26`)
- `shell_single_quote`, line 10.
- `ssh_batch_command(host_alias, cmd) -> std::process::Command`, lines 15–24: `ssh -o BatchMode=yes -- <alias> <cmd>`, with `stdin(Stdio::null())`.
- `run_ssh_batch`, lines 27–47: blocking `.output()`. A non-zero exit becomes an `Err` that carries stderr.
- `contain_remote_path`, lines 50–73: string containment, refusing `..`.
- `default_remote_repo_root(git_url)`, lines 76–96: a **hard-coded `/home/dev/<name>`** (with `/home/dev/repo` for empty or example URLs).

### `packages/tddy-git/src/ssh_worktree.rs`
- `setup_worktree_for_session_over_ssh(ssh_config_host, git_url, session_id) -> Result<String,String>`, lines 8–70. It runs one `set -e` script: clone or init the repo at `default_remote_repo_root`, `fetch --all`, then `git worktree add` at `<repo>/.worktrees/<session_id>` on branch `tddy-<session_id>`. It returns the remote path.

### `packages/tddy-session-split/src/lib.rs`, `docs/module-layout.md`, `Cargo.toml`
- Layout doc: "The crate names no LiveKit crate. A split session's room is hosted, and its agent's join issued, through the `SplitRoomHosting` port in `tddy-daemon-extensions`."
- `SplitSessions` is the handle (`split_ports.rs`); `trait SplitHost: AgentHostCallbacks`.
- Cargo dependencies include `tddy-cli-sessions`, `tddy-daemon-extensions`, `tddy-host-service` (`PeerRouting`), `tddy-coding-agent-specs` and `tddy-worktree-service`. There is no ssh crate.

### `packages/tddy-session-split/src/split_session.rs` (lines 1–470)
- Doc, lines 1–8: "the agent runs on this daemon, its git worktree lives on another … a `TDDY_REMOTE_*` environment pointing `tddy-tools --mcp` at the *codebase* daemon over LiveKit."
- `SPLIT_AGENT_TOKEN_TTL = 86_400s` (line 23).
- `NATIVE_FILESYSTEM_TOOLS` (lines 36–48): Read, Write, Edit, MultiEdit, NotebookEdit, Bash, BashOutput, KillShell, Glob, Grep, LS.
- `SplitAgentWiring { context_dir, extra_args, env, session_pointer }`, lines 51–65.
- `split_session_pointer(remote, context_dir, session_dir)`, lines 71–84: `.tddy/session.json` with `socket: None` and `remote: env_pairs()`.
- `split_agent_participant_identity`, line 99: `split-agent-<session_id>`.
- `split_context_dir = session_dir/context`, line 112.
- `build_split_context_dir`, lines 135–184: preamble plus the guidance fetched from B through a `ContextSource`. A failed fetch fails the start.
- `split_remote_tool_env_from_join`, lines 207–227:
```rust
RemoteToolEnv { daemon_url: String::new(), daemon_socket: None,
    session_id: target.codebase_session_id, session_token: agent_session_token,
    daemon_instance_id: Some(codebase_instance_id), livekit_url: Some(join.url),
    livekit_room: Some(join.room), server_identity: Some(host_identity),
    livekit_token: Some(join.token) }
```
- `colocated_jail_tool_env`, lines 271–293: the HTTP/UDS variant for a sandboxed codebase, with every LiveKit field `None`.
- `SplitAgentRoom::issued_by`, lines 319–335: `split_rooms.agent_join(session_room_name(session_id), split_agent_participant_identity, TTL)`. `host_identity` is `daemon_rpc_identity(local_instance_id)`.
- `SplitSpawnTarget { session_id, codebase_instance_id, codebase_session_id, session_token }`, lines 345–354.
- `prepare_split_agent_wiring(...)`, lines 369–408. This is the resume path's wiring.
- `ssh_client_daemon_instance_id(session_host, codebase_host)`, lines 415–423: "Split: the codebase host holds the checkout and is the client — the agent host has no worktree and must not open SSH."

### `docs/ft/daemon/remote-managed-worktree.md`: the PRD
- Lines 9–40: the two placement axes, `daemon_instance_id` (agent host A) and `codebase_daemon_instance_id` (worktree host B). Tools travel as `ConnectionService/ExecuteTool` (later `StreamExecuteTool`) over LiveKit RPC to `daemon-{B}`.
- Lines 72–76: split requires a room integration, otherwise `FailedPrecondition`.
- Lines 78–115:
- **Co-located SSH:** `ssh_config_host` set means the worktree is created via `setup_worktree_for_session_over_ssh` and exec goes through `RemoteShell`.
- **Split SSH:** B is the OpenSSH client. A never lists A's aliases and never opens ssh.
- ReadLints, LSP and SemanticSearch are unavailable over RemoteShell.
- Lines 241–292 (proto): `StartSessionRequest.codebase_daemon_instance_id = 32`, `requested_session_id = 33`, `SplitAgentPlacement split_agent = 35`, and `SessionEntry.codebase_daemon_instance_id = 29`, `codebase_session_id = 30`.
- Lines 345–355: the `TDDY_REMOTE_*` env contract (`LIVEKIT_URL`, `ROOM`, `TOKEN`, `SERVER_IDENTITY`, `SESSION_ID`, `SESSION_TOKEN`, `DAEMON_INSTANCE_ID`).
- Lines 363–381: `SessionToolTransport` detection order is SandboxIpc, then LiveKit, then DaemonHttp.
- Lines 414–437, session start:
1. A validates the request.
2. A forwards `StartSession{session_type:"workspace"}` to B over the peer forward.
3. A mints the join token and spawns the agent with a context dir and `--settings <context>/.claude/settings.local.json`.
4. A writes `.session.yaml` with `repo_path: None`.
5. The startup window watch (`spawn_startup_grace_period_ms`) follows.
- Lines 444–466: the streamed forward has no overall deadline, only a 30 s idle timeout. Outcomes are `PeerAnswered` / `PeerSilent`.
- Lines 515–520: long tools must use the `block_until_ms: 0` plus `Await` job protocol.
- Lines 522–548: resume re-derives the wiring and mints a fresh token. It reads the roster from B and fails if B is unreachable.
- Lines 586–590: preconditions. Both daemons share `livekit.common_room`, and the GitHub user maps to an OS user on both.

### `packages/tddy-agent-launch/src/opencode_cli_spawn.rs` (lines 100–420): the generic CLI harness start (`start_cli_harness_session`)
- `ssh_config_host` is a field of `CliSessionStart` (line 114) and becomes `ssh_alias` (line 166).
- `provision_cli_worktree(... ssh_alias)` produces `worktree_path`. With an alias, this is a **path on the target**.
- Around lines 308–331:
```rust
let remote_host = Some(ssh_alias).filter(|alias| !alias.is_empty());
...
write_slot_files_on_target(&mut slots, remote_host)?;
let host = PointerHost::of(remote_host);
let pointer = place_session_pointer(wiring.session_pointer.as_ref(), &worktree_path, &host)?;
...
let launched = launch(cli_manager, HarnessSession::new(session_id, &worktree_path, ...), ..., HarnessRun::start(run_in_tmux)).await?;
```
- Line 385: `SessionMetadata.ssh_config_host = remote_host`.
- `cli_branch_starts.rs:231` passes `ssh_config_host: req.ssh_config_host.trim()` into this path. This is the path claude-cli, cursor-cli and any other `<id>-cli` take.

### `packages/tddy-agent-launch/src/opencode_cli_spawn/harness_launch.rs`
- `launch(...)`, lines 89–147. In tmux it uses `plan_host_pty` and `start_harness_in_tmux`; otherwise `launch_host_pty(agent, params, slots, worktree_path, host_pty_session(...))`.
- There is **no ssh branch**: the agent runs locally with `cwd = worktree_path`.

### `packages/tddy-agent-launch/src/spec_launch.rs` (lines 290–329)
- `spawn_host_pty` calls `session.manager.spawn_rendered_main(session_id, session_type, worktree.to_path_buf(), model, argv, env, os_user, on_exit)`. It is local only.
- `SpecLaunchError::SlotFileOverSshTimedOut` is at lines 212–239.

### `packages/tddy-agent-launch/src/cli_session_pointer.rs`
- Lines 68–94:
```rust
pub(crate) enum PointerHost { Local, Ssh { ssh: PathBuf, alias: String } }
impl PointerHost { pub(crate) fn of(remote_host: Option<&str>) -> Self { Self::through(Path::new("ssh"), remote_host) } }
```
This is a second, independent local/ssh enum, separate from tool-engine's `Shell`.
- `place_session_pointer`, lines 111–160. It writes over ssh via `run_over_ssh(..., place_command, contents, SLOT_FILE_OVER_SSH_DEADLINE)`, where `place_command` (lines 165–179) adds `info/exclude` and writes the file under `umask 077` with `cat >`.
- `REMOTE_AGENT_PID_FILE = ".tddy/agent.pid"` and `record_remote_owner` (lines 191–260) are `#[allow(dead_code)]`. The comment says: "today an ssh session's agent is not started on the target, so there is no pid file to read."
- `remove_placed_pointer`, lines 291–314: `rm -f` over ssh.

### `docs/dev/todo/2026-11-07-ssh-agent-launch-remote-pointer-owner.md`
- "No ssh agent launch exists today: `launch_host_pty` runs the agent in a local PTY, and only the slot files and the pointer are written on the target."
- The intended future: the ssh launch script writes `$$` to `.tddy/agent.pid` before `exec`, then `record_remote_owner` is called.

### `packages/tddy-agent-launch/src/slot_file_over_ssh.rs`
- Constants: `SSH_CONNECT_TIMEOUT_SECS = 15`, `SSH_SERVER_ALIVE_INTERVAL_SECS = 5`, `SSH_SERVER_ALIVE_COUNT_MAX = 3`, `SLOT_FILE_OVER_SSH_DEADLINE = 60s`.
- `write_slot_file_over_ssh(alias, path, bytes)`, lines 30–43: `mkdir -p <dir> && cat > <file>`.
- `run_over_ssh(ssh, alias, path, remote, stdin, deadline) -> Result<String, SpecLaunchError>`, lines 68–121. Each pipe is served on its own thread and the child is killed at the deadline.
- `spawn_ssh`, lines 124–134: `Command::new(ssh).args(ssh_options()).arg("--").arg(alias).arg(remote)` with every stdio piped.
- `ssh_options()`: `-o BatchMode=yes -o ConnectTimeout=15 -o ServerAliveInterval=5 -o ServerAliveCountMax=3`. These options are **richer than `tddy-git::ssh_batch_command`**, which sets only BatchMode.

### `packages/tddy-agent-launch/src/cli_agent_steps.rs` (lines 95–123)
- `write_slot_files_on_target(slots, remote_host)` moves `slots.files` to the target, so the local `plan_host_pty` writes nothing at those paths.

### `packages/tddy-agent-launch/src/cli_session_worktree.rs`
- `CliWorktreeStart.ssh_alias`, lines 39–41.
- `remote_worktree()`, lines ~163–180: `spawn_blocking_with_timeout(... tddy_core::setup_worktree_for_session_over_ssh(&alias, &git_url, &session_id))`, which returns a `PathBuf` of the remote path.

### `packages/tddy-session-split/src/svc_spawn_split_agent.rs` (all 447 lines)
- Doc, lines 37–51: one function serves both placements. `room = Some` is split over LiveKit; `room = None` is a sandboxed codebase over this daemon's HTTP. "The argv is deliberately identical either way."
- Flow of `spawn_split_agent`, lines 53–204:
1. create `session_dir`;
2. `record_orchestrator_before_spawn`;
3. `attached_initial_prompt`;
4. `split_agent_tool_wiring`, which resolves the `tddy-tools` path and builds `RemoteToolEnv`, either split-remote or colocated-jail. Its log line at 271–281 prints `relay=uds|http|livekit`;
5. `host_split_room`, which calls `split_rooms.host_remote_worktree(session_id, session_dir, codebase_session_id, codebase_instance_id, snapshots, token_minter, reachable_sessions)`;
6. `split_agent_roster`: `ListSessionAgents` on B, only when `specialized_agents` is non-empty;
7. `split_agent_context_and_args`: the context fetched from B, `build_split_context_dir`, `split_claude_extra_args`;
8. `spawn_split_agent_process`;
9. `record_main_capture`;
10. `startup::ensure_split_agent_started`, which fails with `failed_precondition`;
11. `record_main_capture` again;
12. `write_split_agent_metadata`.
- `agent_session_token_for`, lines 408–418: mints an agent token from the verified caller.
- `bearer_of`, lines 424–437: tells an agent token from a person's.

### `packages/tddy-session-split/src/svc_spawn_split_agent/launch.rs` (all 224 lines)
- `spawn_split_agent_process`, lines 14–72:
1. `hook_token = Uuid::new_v4()`;
2. `record_split_answer_wait`;
3. `write_split_hooks`, which builds `HookCommandParams { tddy_tools_path, daemon_url: advertise_daemon_url, session_id, os_user, hook_token, daemon_socket: Some(agent_tool_socket), answer_wait_secs }`;
4. `render_split_argv`;
5. `split_argv_with_flags`;
6. `split_session_pointer`;
7. `place_pointer_and_spawn`.
- `render_and_write_split_hooks`, lines 133–172: `slots::render(split_agent_spec, SlotInputs{workdir: context_dir, hooks: Some(..), ..})`, then `std::fs::write`. This is **local fs only**, and it is best-effort.
- `render_split_argv`, lines 174–199: `spec.render_run_argv(agent_binary(spec.id), [session_id, chat_id, model, permission_mode, prompt], flags)`.
- `place_pointer_and_spawn`, lines 97–131: `session_pointers.place(&pointer, &context_dir, None)`, then `spawn_split_agent_pty`.

### `packages/tddy-session-split/src/svc_spawn_split_agent/split_agent_pty.rs`
- `spawn_split_agent_pty` calls `cli_manager.spawn_rendered_main(session_id, session_type, context_dir, model, argv, base.agent_env(spec, std::env::vars_os(), &spec.run.env), os_user, on_exit)`. This is a local PTY with a Cleared env base, and the `run.env` part is #906.

### `packages/tddy-session-split/src/svc_spawn_split_agent/startup.rs` (#905)
- `SplitAgentStartup { grace, poll }` is built `from_config`.
- `await_split_agent_startup(handle.pty_done, startup)` returns `Survived` or `Exited`.
- `ensure_split_agent_started` returns `SplitAgentDied { outcome, last_output }`, quoting the last 20 non-empty capture lines with escapes stripped.
- `outcome_of(TaskStatus)` produces strings such as `exit code N` and `stopped by the daemon`.

### Changesets and todo for #905–#907
- `docs/dev/changesets/2026-11-15-split-agent-liveness.md` (#905):
- the split agent's terminal is recorded, the PTY runtime reports real exit codes, and the start fails on an early exit;
- `complexity-svc-spawn-split-agent-spawn-split-agent` grew from 133 to 152 lines.
- `docs/dev/changesets/2026-11-15-split-agent-hooks.md` (#906):
- the spec's `host_agent` variant declares `hooks_flag: --settings`;
- `split_claude_extra_args` adds `--settings <context>/.claude/settings.local.json`;
- `--setting-sources user` and `--strict-mcp-config` stay;
- `run.env` (`CLAUDE_CODE_NO_FLICKER=1`) is carried on start and resume.
- `docs/dev/changesets/2026-11-15-split-session-targets.md` (#907): project-home tools accept a worktree-less split agent half. `TargetRecord.codebase` comes from `SessionEntry` `codebase_*`, and worktree tools redirect to the codebase half (`is_worktree_target_tool` / `retarget_args` in tool-engine).
- `docs/dev/todo/2026-11-15-split-agent-exit-cause-unknown.md`:
- Fix 4 is still unknown: the Claude process exits within seconds. Candidates are the real `tddy-tools --mcp` with LiveKit env, the cleared PTY env, and the long prompt.
- Fixes 6–7 land in #908.

### `packages/tddy-pty` (`src/lib.rs`, `Cargo.toml`, `src/runtime.rs`)
- `lib.rs` doc: "Transport- and host-agnostic PTY core … the I/O pump that plumbs a `portable_pty` master through a `tddy_task::Task` … This crate has no notion of an `os_user`."
- Exports: `PtyEnvBase`, `PtyExitHook`, `PtyReady`, `PtyRuntime`, `PtySpawnSpec`, `PtyControl`, `PtyRegistry`, `strip_resize`, `DEFAULT_TERM_{ROWS=24,COLS=220}`.
- `PtySpawnSpec { argv, worktree_path, session_id, terminal_id, kind, env, env_base }`, runtime.rs lines 47–57.
- `PtyEnvBase::{Inherited (default), Cleared}`, lines 62–70.
- `PtyReady { pid: u32, master: Arc<Mutex<Box<dyn MasterPty + Send>>>, current_size }`, lines 85–89. This hard-wires `portable_pty::MasterPty` into the public API.
- Spawn: `native_pty_system().openpty(...)`, then `slave.spawn_command(cmd)`, then `child.process_id()`, an exit-monitor thread, and the reader-drain grace. `build_pty_command`, lines 420+: `cmd.cwd(&spec.worktree_path)`, the env clear and the GitHub token removal.
- **There is no trait.** `PtyRuntime` is a unit struct that always uses the native PTY.

### `packages/tddy-terminal-rpc/src/pty_runtime.rs`
- The daemon-side `PtySpawnSpec`, lines 47–63, adds `os_user: Option<String>`.
- `PtyRuntime::spawn_with_exit_hook`, lines 82–120: `resolve_final_argv_env` (a `setpriv` privilege drop plus HOME/PATH), then `tddy_pty::PtyRuntime`.

### `packages/tddy-terminal-rpc/src/session.rs`: the transport-agnostic terminal consumer abstraction
- `#[async_trait] pub trait TerminalSession` (lines 51–116) has:
- `capture()`
- `subscribe_stdout()`
- `snapshot_and_subscribe()`
- `subscribe_pty_done()`
- `subscribe_acked_offset()`
- `resizable()` (default true)
- `requires_control()` (default true)
- `async resize(rows, cols)`
- `send_input(data, offset)`
- `trigger_redraw()`
- `TerminalSessionStore::get_terminal(session_id, terminal_id) -> Option<Arc<dyn TerminalSession>>` (lines 118–124).
- This is the **consumer-facing** terminal interface: what bridges stream to the web and LiveKit. It is not the spawn interface.

### `packages/tddy-terminal-rpc/src/pty_relay.rs`
- Four client-side dispatch modes of `PtyRelayConfig`: local PTY, gRPC connect-only, gRPC start-and-connect, and room session via `RoomConnector`.
- This is how `tddy-tools` attaches to a session terminal from outside. It is a client and does not spawn on a remote.

### `packages/tddy-terminal-rpc/src/local_pty_relay.rs`
- `run(argv, cwd, env)`: a local PTY bridged to local stdin/stdout with raw mode and SIGWINCH.

### `packages/tddy-cli-sessions/src/cli_session_manager.rs`, `launch.rs`, `pty_spawn.rs`, `pty_handle.rs`
- `CliSessionManager` (lines 93–131) holds `task_registry`, `pty_registry`, `terminals`, `control`, `managed_workflows`, `exposed_terminals`, `exit_settlements`, `main_recordings`, plus the tmux watchers.
- `spawn_rendered_main(session_id, session_type, worktree_path: PathBuf, model, argv, env, os_user, on_exit)`, launch.rs lines 37–62, calls `spawn_tool_on(..., PtyEnvBase::Cleared, ...)`.
- `spawn_tool_on`, pty_spawn.rs ~lines 71–140, uses `build_pty_spawn_spec` and then `tddy_terminal_rpc::pty_runtime::PtyRuntime::spawn_with_exit_hook`, with a main-recording exit hook. It then runs `await_pty_ready`, `build_pty_handle` and `register_spawned_terminal`.
- `ExitHook`, pty_spawn.rs lines 29–34: `FnOnce(Arc<Mutex<TerminalCapture>>) -> Pin<Box<dyn Future>>`.
- `PtyHandle`, pty_handle.rs lines 24–56, has:
- `terminal_id`, `kind`, `worktree_path`, `model`, `tmux_session`;
- `stdin_tx: mpsc::UnboundedSender<Bytes>`, `stdout_tx: broadcast::Sender<Bytes>`;
- `capture`, `pid: u32`;
- **`master: Arc<Mutex<Box<dyn portable_pty::MasterPty>>>`**;
- `pty_done`, `status_rx`, `current_size`, `task_id`, `channel`.
- Other `pub` API: `start_terminal`, `start_agent_terminal`, `start_tmux_terminal`, `get_terminal`, `list_terminals`, `stop_terminal`, `stop_session`, `kill_all`, `resume(_with_options)`, `expose_terminal_to_livekit`, `ensure_exposed_terminal`, `attach_main_to_tmux`, `reattach_main`, `record_main_capture`, `claim_control` / `verify_control`, and `pty_terminal_service(handle) -> Arc<dyn RpcService>`.

### `packages/tddy-daemon-extensions/src/split_rooms.rs`, `terminal_exposure.rs`, `docs/extension-ports.md`
- `SplitAgentJoin { url, room, token }`.
- `SplitRoomHosting` has `host_remote_worktree(...)` and `agent_join(room, identity, ttl)`. `NoSplitRooms` refuses both.
- `LiveKitSplitRooms` (in `tddy-daemon-livekit`) implements it. `tddy_daemon::runtime::split_room_hosting` picks it when a `livekit:` block exists.
- `TerminalExposure::expose(at: &TerminalAddress, terminal: Arc<dyn RpcService>, metadata_json) -> ExposedTerminal`.

### `packages/tddy-session-tool-client/src/lib.rs` (lines 48–78+)
- `enum SessionToolTransport { SandboxIpc{socket_path}, DaemonUds{..}, DaemonHttp{..}, LiveKit{..} }`. This is the agent→daemon tool transport; there is no SSH variant, by design.

### `packages/tddy-session-lifecycle/src/connection_service/local_exec_tools.rs` (lines 151–190)
- `route_tool`: when `exec_tool_route` gives `ExecToolRoute::HostWorktree`, it reads the `ssh_config_host` metadata and calls `tool_engine::session_shell(worktree_root, ssh_host)` and then `execute_tool_on_shell`. Other routes are `ExecToolRoute::Jail(jail)` and `Refused`.
- This is the **only production consumer** of `session_shell` / `RemoteShell`.

### `packages/tddy-worktree-service/src/remote_git_service.rs`
- `ssh_batch_mode_argv`, lines 283–292: a **third copy** of the ssh argv.
- `remote_pack_shell_command`, lines 296+.
- `PackExecution::{Local{repo_path}, Remote{ssh_config_host, remote_repo_path}}`, line 173.
- `GitChildRelay::spawn_pack_verb`, lines 660–681. Remote runs `spawn_child(ssh_argv, "/", None)`, with the comment "OpenSSH runs as the daemon and must keep agent, config, and PATH".

### `packages/tddy-remote-git-repo/README.md`, `src/lib.rs`
- Git execs this binary in ssh's place (`GIT_SSH_COMMAND=tddy-remote-git-repo`). The remote is `<daemon-instance-id>:<project>`.
- It relays `git-upload-pack` / `receive-pack` stdio over `remote_git.RemoteGitService/Serve` on LiveKit. Credentials are `TDDY_DAEMON_URL` plus a refresh or session token, and the daemon mints the room JWT (`MintLiveKitToken`).
- Modules: `ssh_argv` (`parse_ssh_invocation`, `GitRequest`, `GitVerb`), `credentials`, `daemon_rpc` (`RoomAdmission`), `relay` (`run`, `relay_over_duplex`).
- This is a precedent for "LiveKit as an ssh stand-in", but only for git packs.

### `packages/tddy-askpass/README.md`
- The `SSH_ASKPASS` helper for **daemon-run clones only**. Its env is `TDDY_ASKPASS_URL` and `TDDY_ASKPASS_TOKEN`, and it relays only `Enter passphrase for key '...'`.

### `packages/tddy-agent-launch/src/clone_ssh_env.rs`, `clone_planner.rs`
- `ClonePlan::{Facilitator{ssh_command}, Ssh(CloneSshEnv)}`.
- `agent_socket_for(resolver: &dyn AgentSocketResolver, os_user)`.
- `clone_ssh_env_for(...)` produces `CloneSshEnv` with `SSH_ASKPASS`, `SSH_ASKPASS_REQUIRE=force` and `SSH_AUTH_SOCK`.
- `is_ssh_url`, `resolve_askpass_path`.
- `clone_planner.rs` has `host_ssh_banner()` (`ssh -V`) and `PlannedClone::unprompted`.
- `tddy-spawn/src/clone_env.rs` has `CloneSshEnv::{new, agent_socket, ssh_args, connect_timeout, without_prompting, into_env}` and `OpenSshVersion::supports_forced_askpass`.

### `packages/tddy-host-service/src/ssh_agent.rs`, `ssh_config.rs`, `service/host_keys.rs`; `proto/host.proto` lines 395–415
- `trait AgentSocketResolver { fn socket_for(&self, os_user) -> Result<Option<PathBuf>, String> }` (line 170). The agent is spoken to over its wire protocol through `ssh-agent-lib`.
- `list_ssh_config_hosts(files, os_user)` parses `~/.ssh/config` with `Include`, skipping wildcards.
- The `ListSshConfigHosts` RPC is routed by `daemon_instance_id` (`rpc_served_by_peer`). Its response is `ProbeOutcome`, `failure_reason` and `hosts[]`.

### `packages/tddy-service/proto/session.proto`
- `SessionEntry`: `session_type = 17`, `codebase_daemon_instance_id = 29`, `codebase_session_id = 30`, `ssh_config_host = 33` ("When set, `repo_path` is the worktree on that SSH target"), `managed_codebase = 34`.
- `StartSessionRequest`: `session_type = 7`, `managed_codebase = 17`, `codebase_daemon_instance_id = 32`, `requested_session_id = 33`, `split_agent = 35` (`SplitAgentPlacement { session_id, agent_daemon_instance_id }`, line 466), `ssh_config_host = 38` ("Implies managed-codebase semantics … never a fallback to LocalShell"), `sandboxed_codebase = 39`.
- There is no "session kind = split" enum. Split is inferred from a non-empty `codebase_daemon_instance_id`.

### `packages/tddy-changeset/src/session_metadata.rs`
- Lines 87–123: `.session.yaml` carries `codebase_daemon_instance_id`, `codebase_session_id`, `agent_daemon_instance_id` and `agent_session_id` (the back-pointer on B's workspace session), plus `ssh_config_host` and `managed_codebase`.
- `reads_host_paths(meta)`, lines 224–245, is false for split, ssh, managed, sandbox and workspace sessions.

### Web
- `packages/tddy-session-create-web/src/CreateSessionSshConfigSelect.tsx`: `sshConfigListDaemonId(sessionHost, codebaseHost)` returns the codebase host when split and the session host otherwise. The component uses `HostService.ListSshConfigHosts`.
- `useCreateSessionPlacement.ts` (lines 50–207) holds `codebaseDaemonInstanceId` and `sshConfigHost` state and the `isSplit` predicate.
- `packages/tddy-session-web/src/SessionDrawerGroups.tsx:40-45`: the codebase-host badge.
- `packages/tddy-chat-web/src/sessionRosterHalf.ts:27`.

### `docs/ft/daemon/claude-cli-session.md` (lines 535–590)
- "ssh checkout. On an ssh checkout the slot files are written on the target over ssh (bounded by the 60 s slot-write deadline …)". The pointer is written on the target, and "The remote agent is not yet recorded as an ssh pointer's owner".
- The agent env base includes `SSH_AUTH_SOCK`.

### `packages/tddy-agent-backend/src/claude_hooks.rs`
- The hook `command` is a shell string run by `/bin/sh -c`:
`'<tddy_tools_path>' session-hook --session … --daemon <url> [--daemon-socket '<sock>'] --os-user … --hook-token … --event …`
- Awaiting hooks (PermissionRequest, AskUserQuestion) carry a timeout of `answer_wait_secs` plus a margin.
- So every hook depends on the `tddy-tools` binary and on reaching the daemon's URL or socket.

### Prior discovery: `docs/dev/changesets/2026-09-14-split-ssh-initial-discovery.md`
- It records the makers-lt analogue: `abstract class Shell { execCommand; putFileContent; getFileContent }`, `LocalShell`, and `RemoteHost` building `ssh … -p … target cmd`.
- It concludes that SSH belongs in the daemon-side engine, not in `tddy-tools` (because of the jail).

### Todo heads read
- `2026-08-14-a-split-session-s-room-is-not-re-opened-when-the-daemon-restarts.md`: there is no startup sweep, so a resumed split agent's `connect_livekit_client` times out after 10 s.
- `2026-09-06-loading-a-key-needs-a-privileged-socket-path.md`: a supervised daemon can reach only its own account's ssh-agent. This affects RemoteShell.
- `2026-10-17-daemon-git-over-ssh-beyond-the-clone-cannot-ask-for-a-passphrase.md`: askpass covers only the auto-clone.
- `2026-09-06-ssh-agent-bounds-stop-at-the-conversation.md`: the ssh-agent probe's bounds stop at the conversation.
- `2026-10-25-remote-codebase-two-daemon-suite-not-run.md`: the real LiveKit hop is unverified for coagents.
- `2026-08-13-tddy-coder-remote-never-completes-a-session-bootstrap.md`.

---

#### 3. Grep / glob

| Pattern | Scope | Notable hits |
|---|---|---|
| `RemoteShell\|remote_shell\|PtyBackend\|trait \w*Shell` | rs/ts/tsx/md/proto, repo-wide | `tddy-tool-engine/src/shell.rs:45` `pub trait Shell`, `:98` `pub struct RemoteShell`; `tests/remote_shell_exec_acceptance.rs`; `split_session.rs:455`; `docs/ft/daemon/remote-managed-worktree.md:87,99,109,114`; the 2026-09-14 changesets. **No `PtyBackend`, and no other Shell trait.** |
| `ssh\|GIT_SSH\|askpass` | `tddy-session-split/src` | only `split_session.rs:410-457` (`ssh_client_daemon_instance_id` and its tests) and `service_util.rs:116` (`ssh_config_host: None`) |
| `ssh_config_host\|setup_worktree_for_session_over_ssh\|run_ssh_batch\|ListSshConfigHosts\|session_shell(` | packages | agent-launch (`cli_branch_starts.rs:231`, `opencode_cli_spawn.rs:114/166/385`, `cli_session_worktree.rs:174`, `cli_session_pointer.rs:78`, `svc_signal_delete_session.rs:221-231`, `project_home_start_spec.rs:63` refuses ssh for project-home, `opencode_cli_spawn/resume.rs:207`, `session_coordinate_handlers.rs:427`); tool-engine `shell.rs`, `start_session_args.rs:57`, `start_options.rs:112`; host-service `ssh_config.rs`, `host_keys.rs:197-243`; `session.proto:375,714`; `host.proto:78`; `tddy-git/src/ssh_worktree.rs`, `ssh_exec.rs:27`; `local_exec_tools.rs:166` |
| `execute_tool_on_shell\|session_shell\|RemoteShell::new\|LocalShell::new` | packages | production: `tddy-session-lifecycle/src/connection_service/local_exec_tools.rs:166-167` only; the rest are tests |
| `PointerHost\|write_slot_files_on_target\|remote_host\|fn launch\b\|"ssh"\|Command::new("ssh")\|-tt\|-t` | agent-launch, cli-sessions, pty, core | `PointerHost` (`cli_session_pointer.rs`), `slot_file_over_ssh.rs:37`, `clone_planner.rs:27` (`ssh -V`), tmux `-t` only. **No `ssh -t`/`-tt` anywhere.** |
| `Command::new("ssh")\|ssh_batch_command\|ssh_batch_mode_argv\|run_ssh_batch\|run_over_ssh(` | non-test packages | `tddy-git/src/ssh_exec.rs:16`, `tddy-agent-launch/src/slot_file_over_ssh.rs:68/124` plus `cli_session_pointer.rs:123,240,295`, `clone_planner.rs:27`, `tddy-worktree-service/src/remote_git_service.rs:283,675`, `tddy-tool-engine/src/shell.rs:138`, `tddy-vm/src/qemu.rs:747,771` (VM guest), `tddy-vm-testkit/src/guest.rs:335` |
| `GIT_SSH_COMMAND\|SSH_ASKPASS\|tddy-askpass` | packages | `tddy-projects/src/project_provision.rs:17,142,154`; `tddy-session-agents/src/session_agent_clone.rs:464,830,870,1139`; `svc_provision_agent_clone.rs:38`; `tddy-agent-launch/src/clone_ssh_env.rs:84,107`, `clone_planner.rs:305-361`, `svc_ensure_project_available_for_start.rs:142,213`, `launch_ports.rs:113`; agent-launch tests `clone_askpass_acceptance.rs`, `clone_ssh_env.rs` |
| `pub struct SessionMetadata` | packages | `tddy-changeset/src/session_metadata.rs`, `tddy-testing-commons/src/builders.rs`, `tddy-coder/src/session_participant/metadata_publisher.rs` |
| `sshConfigHost\|codebaseDaemonInstanceId\|sshConfigListDaemonId` | `tddy-*web*/src` | `tddy-session-create-web` (`CreateSessionSshConfigSelect`, `CliFields`, `PaneState`, `Pane`, `Placement`, `createSessionRequest`, `ManagedCodebaseFields`, `useStartingHost`, plus 2 tests), `tddy-session-web/SessionDrawerGroups.tsx`, `tddy-chat-web/sessionRosterHalf.ts` |
| `pub trait AgentSocketResolver` | host-service | `ssh_agent.rs:170` |
| `spawn_pack_verb` | packages | `remote_git_service.rs:542,660`; tests `remote_git_ssh_{clone,serve}_acceptance.rs` |
| `HookCommandParams` | packages | `tddy-agent-backend/src/claude_hooks.rs:27` (also `muse_hooks.rs`) |
| `ls docs/dev/todo \| grep ssh\|split\|pty\|shell\|remote` | todo | 28 entries; the relevant ones are listed in §2 |
| `ls docs/dev/changesets \| grep split\|ssh\|pty\|shell\|remote\|transport` | changesets | `2026-09-14-{split-ssh,ssh-exec,ssh-config-hosts,remote-git-ssh}*`, `2026-10-20-satellite-12-split-transport`, `2026-11-07-split-remote-env-via-pointer`, `2026-11-15-split-{agent-hooks,agent-liveness,session-targets}`, `2026-07-01-rpc-multi-transport` |
| grep docs for `ssh_config_host\|RemoteShell\|split session\|split placement` | docs/ft, packages/*/docs | `remote-managed-worktree.md`, `claude-cli-session.md`, `remote-codebase-mode.md`, `remote-git-repo.md`, `clone-ssh-passphrase.md`, `session-room.md`, `terminal-sessions.md`, `project-concept.md`, web changelogs (e.g. `2026-08-14-choosing-which-daemon-holds-a-session-s-codebase.md`), `amendments/PRD-2026-08-31-split-sandbox-orchestration.md` |

---

#### 4. Findings

### A. What a "split session" is
- A split session is a **two-daemon placement over LiveKit, not SSH**. `StartSessionRequest.daemon_instance_id = A` runs the agent; `codebase_daemon_instance_id = B` holds the worktree. A daemon with no LiveKit `split_rooms` refuses it.
- End-to-end start on A, in order:
1. Validate: managed codebase, claude-cli, an eligible B.
2. `SplitAgentRoom::issued_by`: get the room join for `session_room_name(session_id)` as `split-agent-<id>`, with a 24 h TTL.
3. Stream-forward `StartSession{session_type:"workspace", requested_session_id, split_agent}` to B over the common room. B clones or creates the worktree, and runs its own `ssh_config_host` setup if one was set.
4. Run `spawn_split_agent`:
   - mint an agent token;
   - build `RemoteToolEnv` (LiveKit fields only);
   - host the session room measuring B's worktree (`host_remote_worktree` polling `GetWorktreeSnapshot`);
   - read the roster and context guidance from B;
   - build `<session_dir>/context`;
   - write the hooks file locally at `<context>/.claude/settings.local.json`;
   - render the claude argv from the spec, adding `--strict-mcp-config --setting-sources user`, `--settings <hooks>`, the `--disallowedTools` natives, and `--mcp-config` registering `tddy-tools --mcp` with the `TDDY_REMOTE_*` env;
   - place `.tddy/session.json` in the context dir.
5. Spawn the agent in a **local PTY** via `CliSessionManager::spawn_rendered_main` (cleared env plus allow-list plus `run.env`).
6. Run the startup-window watch (#905).
7. Write `.session.yaml` with `repo_path: None` plus the `codebase_*` fields.
- Tool calls: `tddy-tools --mcp` on A goes over LiveKit `StreamExecuteTool` to `daemon-{B}`. B's `route_tool` reaches `HostWorktree`, which uses `session_shell(root, ssh_config_host)`: LocalShell or RemoteShell on B.
- Hooks: the split agent's hooks run on A and report to A's agent-tool socket (`daemon_socket: Some(agent_tool_socket)`) or A's advertised URL.
- What runs where:
- The split model assumes a **full tddy-daemon on B** (B serves the workspace session, ExecuteTool, GetWorktreeSnapshot, ListSessionAgents and the context fetch).
- On A it assumes `tddy-tools` and `claude`.
- Nothing is assumed on an SSH target T except `sh`, `git`, `cat`, `base64`, `mkdir`, `rg`, `find` and `sed`, since RemoteShell's Grep needs `rg` on T.
- The web and daemon represent a split through `SessionEntry` / `.session.yaml` fields only (`codebase_daemon_instance_id`, `codebase_session_id`, and the back-pointer `agent_daemon_instance_id` / `agent_session_id` on B). There is no session-kind enum. #907 added `TargetRecord.codebase` so the project-home tools redirect worktree tools to B.

### B. How SSH is used today (`ssh_config_host`)
- **Addressing:** an explicit OpenSSH `Host` alias from the code-managing daemon user's `~/.ssh/config`, listed by `ListSshConfigHosts` (routed to B when split; the web helper is `sshConfigListDaemonId`).
- **Authentication:** whatever OpenSSH resolves from config plus the ssh-agent in the daemon's environment, with `BatchMode=yes` so there is never a prompt.
- RemoteShell and `remote_git_service` inherit the daemon's env; nothing sets `SSH_AUTH_SOCK` explicitly.
- Only clones use `AgentSocketResolver`, `CloneSshEnv` and `tddy-askpass`.
- The supervised daemon can reach only its own agent (todo 2026-09-06).
- **The client is always the daemon process** that holds the checkout. `tddy-tools` in a jail never opens SSH.
- What is done over SSH, each as a one-shot non-interactive `ssh … -- alias 'cmd'`:
1. worktree setup (`setup_worktree_for_session_over_ssh`, a hard-coded `/home/dev/<repo>` root);
2. exec-catalog tools through `RemoteShell` (`spawn_blocking(run_ssh_batch)`);
3. slot files (hooks settings and similar) plus the session pointer, via `run_over_ssh` with ConnectTimeout, ServerAlive and a 60 s deadline;
4. git pack verbs for `RemoteGitService.Serve` (`PackExecution::Remote`, async `tokio::process`, piped stdio streamed as frames);
5. pointer removal on exit and delete.
- **There is no PTY over ssh and no `ssh -tt`.** The coding agent is started with `launch_host_pty` / `spawn_rendered_main` as a **local** `portable_pty` child whose `cwd` is the worktree path. For an ssh checkout that path is the *remote* path string.
- `record_remote_owner` / `REMOTE_AGENT_PID_FILE` are dead code waiting for an "ssh agent launch" (todo 2026-11-07).
- How portable-pty treats a cwd that does not exist locally was not checked in this pass.
- The project-home main refuses `ssh_config_host` (`project_home_start_spec.rs:63`).
- **No remote agent or daemon is assumed on an SSH target.** It is a bare `sh`. The hook command still requires `tddy-tools` plus a reachable daemon URL or socket, which is the open "hooks without remote installation" constraint in the WIP doc.

### C. Existing abstractions relevant to a transport-agnostic RemoteShell

| Abstraction | Location | Shape | Gap |
|---|---|---|---|
| `Shell` trait plus `LocalShell` / `RemoteShell` | `tddy-tool-engine/src/shell.rs` | read, write, run(non-interactive) → `std::process::Output`; `kind()` enum | Exec catalog only. No streaming, no stdin, no PTY, no background jobs. Dispatch is by `ShellKind`, not by trait. Errors are untyped strings. Used only by `local_exec_tools.rs:166`. |
| `PointerHost::{Local, Ssh{ssh, alias}}` plus `run_over_ssh` | `tddy-agent-launch/src/cli_session_pointer.rs`, `slot_file_over_ssh.rs` | write a file or run a command with stdin, bounded deadline | A second local/ssh enum. Better ssh options than tool-engine's. |
| `PackExecution::{Local, Remote}` plus `ssh_batch_mode_argv` | `tddy-worktree-service/src/remote_git_service.rs` | streamed piped child | A third argv copy. Async, piped. |
| `ssh_batch_command` / `run_ssh_batch` / `setup_worktree_for_session_over_ssh` | `tddy-git/src/ssh_exec.rs`, `ssh_worktree.rs` (via `tddy_core`) | blocking std `Command` | Only BatchMode, no connect timeout. Hard-coded remote root. |
| `tddy_pty::PtyRuntime` / `PtySpawnSpec` / `PtyReady` / `PtyRegistry` | `tddy-pty` | local `portable_pty` spawn plus I/O pump into a `tddy_task::Task` | No trait. `PtyReady.master` and `PtyHandle.master` are concretely `Box<dyn portable_pty::MasterPty>`, which leaks into `PtyRegistry` resize. |
| daemon `PtyRuntime` (os_user / `setpriv`) | `tddy-terminal-rpc/src/pty_runtime.rs` | wraps `tddy_pty` | Impersonation is the local host's only. |
| `CliSessionManager::spawn_rendered_main` / `PtyHandle` / `ExitHook` | `tddy-cli-sessions` | the session terminal registry, capture, control lease, LiveKit exposure | The natural seam where a "remote PTY" would plug in. Every agent launch, split included, goes through it. |
| `TerminalSession` / `TerminalSessionStore` traits | `tddy-terminal-rpc/src/session.rs` | consumer-side live terminal (capture, stdout broadcast, `pty_done`, resize, input, ack) | Already transport-agnostic and already allows `resizable() = false`. A remote PTY could implement this for the bridges. |
| `TerminalExposure`, `SplitRoomHosting`, `SessionToolTransport` | `tddy-daemon-extensions`, `tddy-session-tool-client` | LiveKit as one implementation behind ports | The precedent for "LiveKit is one transport among several". |
| `tddy-remote-git-repo` | crate | LiveKit as an `ssh` stand-in for git packs | A precedent for tunnelling a stdio exec over LiveKit. |

### D. What a transport-agnostic "RemoteShell" would have to replace or wrap
1. **Unify the three or four ssh spawn sites behind one trait or port:**
 - tool-engine `RemoteShell::ssh_run` (`run_ssh_batch`);
 - `tddy-git` `ssh_batch_command` (worktree setup);
 - agent-launch `run_over_ssh` (slot files, pointer, owner);
 - worktree-service `ssh_batch_mode_argv` (git packs, streamed).

 They need one-shot exec (with stdin and a deadline), streamed exec (piped duplex, as for git packs), file write and read, and the containment root.
2. **Add an interactive PTY capability**, which does not exist today. Agent launches (`launch_host_pty`, `spawn_split_agent_pty`, `start_harness_in_tmux`) all end in `CliSessionManager::spawn_tool_on` → `tddy_terminal_rpc::pty_runtime::PtyRuntime` → `tddy_pty` (native `openpty`). A remote shell would have to supply:
 - a spawn returning stdin, stdout bytes, exit status, a pid or identifier, and resize;
 - a replacement for `PtyReady.master` / `PtyHandle.master` / `PtyRegistry` (all `portable_pty::MasterPty`-typed). One option is a local PTY running `ssh -tt alias 'cd … && exec agent'`, where the transport sits inside the argv; that would keep `MasterPty` but not be transport-agnostic.
 - `cwd` and env as part of the remote command. With a cleared env plus allow-list, a remote env must be passed as an `env -i …` prefix, because ssh does not forward env.
3. **Rework hooks.** The hooks file is rendered and written locally (`render_and_write_split_hooks` uses `std::fs::write`; on ssh checkouts `write_slot_files_on_target` moves it). The hook command calls a local `tddy-tools session-hook --daemon <url> --daemon-socket <sock>`. For a bare remote, the command shape (`claude_hooks.rs` `hook_entry_for_event`) needs a transport-specific carrier. See the WIP constraint: append-to-file or FIFO, a reverse-forwarded socket, or in-band PTY markers. Blocking hooks (PermissionRequest, AskUserQuestion) wait up to `answer_wait_secs`.
4. **Generalise the session pointer and owner**: `PointerHost`, `record_remote_owner` and `REMOTE_AGENT_PID_FILE` are ready for an ssh launch, but nothing calls them yet.
5. **Rework authentication and addressing.** SSH today is "an alias from `~/.ssh/config` plus the inherited ssh-agent plus BatchMode". There is no data model for other transports (Coder, LiveKit) beyond the `ssh_config_host: String` proto and metadata field, the `ListSshConfigHosts` listing, and the web dropdown, all of which would need a transport-tagged target. `default_remote_repo_root` is hard-coded to `/home/dev/<name>`.
6. **Split stays separate.** Split's agent↔B leg is already LiveKit via `SessionToolTransport::LiveKit` and `SplitRoomHosting`. A RemoteShell would wrap only the B→T leg (`session_shell` on B) and the agent-launch leg. `ssh_client_daemon_instance_id` encodes "B is the client when split".

## Exploration 4: hooks adapters per agent — 2026-11-15

**Agent**: parent Grep/Read
**Scope**: how each coding-agent spec declares and renders lifecycle hooks

### Sequence

1. Grep `hooks?` in `packages/tddy-coding-agent-specs/specs` — which specs declare a hooks adapter.
2. Grep `session-hook|hook_entry_for_event|fn render.*hooks` in `packages/*/src` — where commands are built.
3. Read `packages/tddy-coding-agent-specs/src/slots/hooks.rs` 1–140 — the adapter trait.
4. Grep `fn |session-hook` in `tddy-agent-backend/src/{claude,muse,cursor}_hooks.rs` — the builders.

### Grep / glob

| Tool | Pattern | Path scope | Notable hits |
|---|---|---|---|
| Grep | `hooks?` | `tddy-coding-agent-specs/specs` | `claude.yaml:50 hooks: { adapter: claude-settings-local }`; `cursor.yaml:56 cursor-hooks-json`; `muse.yaml:51 muse-config-arg`; `claude-acp`, `codex`, `codex-acp`, `opencode`, `warp`, `oz`: `unsupported` |
| Grep | `session-hook` | `packages/*/src` | `tddy-agent-backend/src/{claude_hooks,muse_hooks,cursor_hooks}.rs`, `tddy-coding-agent-specs/src/slots/{hooks,cursor}.rs`, `tddy-tools/src/session_hook.rs`, `tddy-session-split/src/svc_spawn_split_agent/launch.rs` |

### Inspected files

#### `packages/tddy-coding-agent-specs/src/slots/hooks.rs`

**Why**: is the hook command agent-specific or shared?
**Excerpt**:

```rust
pub enum RenderedHooks { File(PathBuf, Vec<u8>), Argv(Vec<String>) }
pub enum HooksDelivery { File, Argv }
/// One way of wiring an agent's lifecycle hooks to `tddy-tools session-hook`.
pub trait HooksAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn delivery(&self) -> HooksDelivery { HooksDelivery::File }
    fn render(&self, workdir: &Path, params: &HookCommandParams<'_>) -> (PathBuf, Vec<u8>);
    fn installed_events(&self) -> &'static [&'static str] { &[] }
    fn try_render(&self, workdir: &Path, params: &HookCommandParams<'_>) -> Result<RenderedHooks, SlotError> { … }
}
```

#### `packages/tddy-agent-backend/src/claude_hooks.rs`

**Why**: what the command string is.
**Excerpt**:

```rust
/// A hook `command` is a **shell string**, not an argv: Claude Code and Cursor both hand it to …
pub(crate) fn hook_entry_for_event(…)
    "{} session-hook --session {} --daemon {}{} --os-user {} --hook-token {} --event {}",
```

`muse_hooks.rs:41 build_muse_hooks_config_value(p: &HookCommandParams)` and
`cursor_hooks.rs:23 build_cursor_hooks_settings(p: &HookCommandParams)` format the same command.

### Findings

- Three adapters install hooks today: `claude-settings-local` (file), `cursor-hooks-json` (file),
  `muse-config-arg` (argv). Six specs are `unsupported`.
- All three build a **shell string** from one `HookCommandParams`; the only tddy-specific part is
  the `tddy-tools session-hook …` prefix. Replacing that prefix with an inline `sh` append, selected
  by the session's target, makes every adapter remote-capable with no per-agent work.
- Delivery differs: file adapters need the rendered file written on the remote (slot files already
  go over ssh); the argv adapter's entries join the remote command line.
- Muse and Cursor adapters refuse params without a daemon socket (`SlotError::NoDaemonSocket`); a
  remote carrier needs that refusal to recognise the remote case.
