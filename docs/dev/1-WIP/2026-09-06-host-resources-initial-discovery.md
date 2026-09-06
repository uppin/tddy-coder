# Initial discovery — `host-resources` (#hosts-screen 3/8)

**Node:** memory, load average and core count in host telemetry

**Status:** complete for planning.

## Exploration 1 — the whole-work discovery behind the `#hosts-screen` stack

The full dump that produced the eight-node decomposition. Node-specific follow-up follows it.

**Work:** a new "Hosts" screen in `tddy-web` listing connected and previously-seen hosts with their
connectivity and capability status — ssh-agent availability + loaded keys (with add-key + passphrase
prompt forwarded through the UI), GitHub CLI install/auth status, git user config, VNC/RDP
availability and connect action, and live resource telemetry (disk, memory, CPU count, CPU load)
pushed over a server-streaming RPC.

**Base:** planned on top of `feature/optional-livekit/lazy-session-room` (see § Base decision).

## Combined conclusions

_(filled in as explorations land)_

## Product decisions (interview, 2026-09-06)

| Question | Decision |
|---|---|
| Stack base | Base on `feature/optional-livekit/lazy-session-room` **for now**. That branch is the top node (9/9) of the still-open `#optional-livekit` stack (#437→#451) and is expected to merge to `master` soon; **when it does, the bottom node of this stack is repointed onto `master`** (`/repoint`) and the rest cascade. Planned as a *separate registered stack*, not an extension of `#optional-livekit`. |
| VNC / RDP | **In-app viewer inside `tddy-web`** — not a hand-off to an external client. Availability reporting and the viewer are therefore separate capabilities. |
| ssh-agent passphrase | **Forward, never persist.** Prompt in `tddy-web`, carry over the existing RPC transport, hand to the agent-add on the host, drop immediately. Never on disk, never logged, never retained in daemon state past the call. No "remember" option. |
| Previously-seen hosts | **Daemon-side registry** — a durable record of hosts the daemon has seen, surviving a browser reload and shared across browsers. Not `localStorage`. |

### Base consequence to carry through planning

Every node of this stack sits on top of 9 unmerged PRs. Nothing here can land until `#optional-livekit`
lands. The bottom node's base is `feature/optional-livekit/lazy-session-room` and **must be repointed to
`master`** once #451 merges — that repoint is manual and nothing performs it automatically.

## Exploration 1 — product docs (`docs/ft/web/`), read directly

Much of the requested surface is **already built at a different scope**. The Hosts screen is largely
assembly + extension, not greenfield. Evidence:

| Existing doc | What it already gives us |
|---|---|
| `app-shell.md` | `AppShell` (`packages/tddy-web/src/components/shell/AppShell.tsx`) owns top chrome for every routed screen: `DaemonNavMenu` hamburger, title, `DaemonSelectorConnected`, avatar. A new screen supplies only body content, `variant="scroll"`. |
| `url-state-routing.md` | Hash routing `#/<path>?<params>`, modules `routing/{appLocation,useAppLocation,appRoutes,selectedHost}.ts`. A `#/hosts` path slots into that grammar. |
| `projects-screen-multi-host.md` | **The template for this work.** `/projects` = `ProjectsAppPage` (data container) + `ProjectsScreen` (presentational) + a `DaemonNavMenu` entry, mirroring `VmsAppPage` / `WorktreesAppPage`. Also states the current definition of a host: **"A host is a daemon instance (`tddy-daemon`)"**, discovered via `ListEligibleDaemons`. |
| `host-stats-footer.md` | **Live host telemetry already ships**: available disk space + per-core CPU usage for the selected daemon, in a screen-level footer on the sessions drawer. So disk + CPU are extensions, not new capabilities. |
| `worktree-disk-usage-streaming.md` | Names an existing server-streaming RPC **`StreamHostStats`** and shows the house pattern for a new one (`StreamWorktreeStats(...) returns (stream ...)`, first frame = snapshot, then incremental frames). |
| `vnc-sessions.md` | **An in-browser VNC viewer already exists**, session-scoped: VNC targets (label, host:port, password), a VNC inspector tab, a full-screen overlay streaming the desktop with mouse/keyboard forwarded back. Gated on a connection that carries media. |
| `screen-sharing-sessions.md` + `changelog/2026-06-26-screen-sharing-tab-with-vnc-and-rdp-protocol-selector.md` | A screen-sharing tab with a **VNC *and* RDP protocol selector** already exists. |

### Consequences for decomposition

- **Telemetry** (disk, memory, CPU count, CPU load) extends `StreamHostStats` rather than inventing a
  feed. Memory + CPU count + load average are the genuinely new fields.
- **VNC/RDP in-app viewer** is *re-scoping an existing viewer from session to host*, not building one.
  That is a far smaller node than a greenfield viewer — confirm against the code before sizing it.
- **ssh-agent** and **`gh` CLI / git identity** are the two areas with no obvious precedent; they are
  the likely centre of gravity of this stack.
- A precedent exists for prompting for a passphrase in the UI (`vnc-sessions.md` AC: "I am prompted for
  a passphrase the first time I add a target with a password"), though that one *stores* it encrypted,
  whereas our ssh-agent decision is forward-and-drop.

## Exploration 2 — the host concept in `tddy-web` and `tddy-daemon`

### What a host is

Two co-existing TS vocabularies for the same machine, with exactly one conversion module between them:

- **`HostDescriptor`** — `packages/tddy-web/src/rpc/hostDirectory/types.ts:24-38`:
  `hostId`, `label`, `sourceId`, `reposBasePath?`, `maxAttachmentBytes?`.
- **`DaemonHost`** — `packages/tddy-web/src/lib/participantRole.ts:12-21`:
  `instanceId`, `label`, `reposBasePath?`, `maxAttachmentBytes?`. This is the vocabulary the screens
  actually read (`DaemonSelector`, `ProjectsAppPage`, the Start-Session form).
- Conversion: `hostDescriptorOf` / `daemonHostOf` — `rpc/hostDirectory/daemonHost.ts:23`, `:36`.
- `HostDirectorySource` / `HostDirectory` — `types.ts:48-58`, `:61-84`.

Daemon side: `EligibleDaemonInfo { instance_id, label }` — `packages/tddy-daemon/src/multi_host.rs:11-14`;
wire advertisement `DaemonAdvertisement { instance_id, label, repos_base_path, max_attachment_bytes }` —
`packages/tddy-daemon/src/livekit_peer_discovery.rs:117-131`.

### Discovery: two sources, merged by precedence

Assembled in `useDirectorySources` — `packages/tddy-web/src/rpc/selectedDaemon.tsx:153-193`.

1. **LiveKit source** (`liveKitSource.ts`, id `"livekit"`) — participants of the common room, parsed by
   `parseDaemonAdvertisement` (`participantRole.ts:39`). Unconfigured LiveKit → `idle`, **not** `error`.
2. **Serving source** (`servingSource.ts`, id `"serving"`) — the daemon that served the page, from
   `/api/config`'s `daemon_instance_id`. Names a host; does **not** claim a wire.

Merge `mergeHostDirectory` — `useHostDirectory.tsx:25`; dedup by `hostId`, **first source wins**.

### Reaching a host is a separate registry from naming it

`rpc/connections/types.ts`: `ConnectionCapability = "rpc" | "media" | "presence"` (`:33`),
`ConnectionStatus = "idle" | "connecting" | "connected" | "error"` (`:51`),
`HostConnection` (`:60-113`) with `clientFor<S>(service)`, `transport()`, `openSession(...)`,
`ConnectionProvider` (`:121-134`). Registry hooks in `rpc/connections/registry.tsx`:
`useHostConnection(hostId)` (`:195`), `useHostClient(service, hostId)` (`:207`).

**Each host is its own `tddy-daemon` with its own connection.** A fleet-wide screen therefore needs one
client per host — this is the central design constraint of the whole stack.

### ⚠ "Previously seen" does not exist — anywhere

- Host presence is **entirely live and ephemeral**. The LiveKit source rebuilds its list from the current
  participant roster on every room event; the serving source is one constant entry.
- **No persistence of a host list.** A `localStorage` / `sessionStorage` sweep of `packages/tddy-web/src`
  finds only auth tokens, OAuth state, debug mask, screen id, presence identity, and the *selected* host id
  (`SELECTED_DAEMON_STORAGE_KEY = "tddy_selected_daemon"`, `sessionStorage`, per-tab — `routing/selectedHost.ts:14`).
- `resolveSelectedDaemonInstanceId` (`selectedHost.ts:38-51`) **only returns ids still present in `daemons`** —
  a departed host is dropped, never remembered.
- Rust side identical: `CommonRoomPeerRegistry` (`livekit_peer_discovery.rs:267-269`) is an
  `RwLock<HashMap<..>>` replaced wholesale from each room snapshot (`sync_from_room`, `:277`). In-memory,
  membership-authoritative, **no disk persistence**.
- Nothing records host disappearance, last-seen timestamps, or an offline host row.

→ The **daemon-side registry** the interview chose is genuinely new construction, not a surfacing job.
  It is also the natural bottom node: everything else on the screen hangs off a host row.

### Host stats today: real, but narrow and single-host

- Proto — `packages/tddy-service/proto/connection.proto`:
  `rpc StreamHostStats(StreamHostStatsRequest) returns (stream HostStatsEvent);` (`:262`),
  `HostStatsEvent { HostCpuStats cpu = 1; HostDiskStats disk = 2; }` (`:2250`),
  `HostCpuStats { repeated float per_core_percent = 1; }` (`:2254`),
  `HostDiskStats { uint64 available_bytes; uint64 total_bytes; string project_dir; }` (`:2258`).
- Handler — `packages/tddy-daemon/src/connection_service.rs:16529-16595`; immediate emit on subscribe,
  then independent `cpu_tick` (5 s) and `disk_tick` (60 s) timers, each pushing a **full** event.
  Provider injected as `host_stats: Arc<dyn HostStats>` (`:1110`), builder overrides `with_host_stats` (`:2016`),
  `with_host_stats_intervals` (`:2032`).
- Trait — `packages/tddy-daemon/src/host_stats.rs:57-62`:
  `HostStats { fn cpu_per_core_percent(&self) -> Vec<f32>; fn disk_for_project_dir(&self) -> DiskUsage; }`,
  impl `SysinfoHostStats` (`:69`), long-lived so `sysinfo` per-core deltas are real.
- Hook — `packages/tddy-web/src/rpc/useHostStats.ts:38`. **Streaming**, via
  `useDaemonClient(ConnectionService)` — i.e. **the selected daemon only**.

**Missing from the stats surface: memory, CPU count, load average.** (`sysinfo` is already the dependency,
so the daemon-side additions are cheap.) **There is no per-host stats fan-out** — that is the new capability.

### `useHostFanOut` cannot carry this

`packages/tddy-web/src/rpc/useHostFanOut.ts:103` — `HostReader.read(client, id, signal): Promise<readonly T[]>`
is **unary-shaped**. A per-host *streaming* screen is not expressible through it. Its own doc comment
(`:19-23`) notes `components/models/useModelRegistryFanOut` had to be written separately for a composite
case — the closest precedent for a Hosts screen wanting several readings per host.

### Where a Hosts screen slots in — 5 touch points

Routing is hand-rolled hash routing, **no react-router**. Dispatch is a single ternary chain in
`packages/tddy-web/src/index.tsx:480-500`.

1. `routing/appRoutes.ts` — `HOSTS_ROUTE = "/hosts"` + `isHostsPath`.
2. `components/hosts/HostsAppPage.tsx` (data/RPC, wraps `AppShell`) + `HostsScreen.tsx` (presentational).
   Template: `VmsAppPage.tsx:30` / `VmsScreen.tsx`; capability-gated variant: `LiveKitAppPage.tsx:29`.
3. `index.tsx` — one import + one branch in the ternary chain.
4. `components/shell/DaemonNavMenu.tsx` — one `Button role="menuitem"` with a `data-testid`
   (existing ids follow `shell-menu-<screen>`).
5. `packages/tddy-web/docs/hosts-screen.md` — the repo documents every screen.

`PARAM_HOST` (`routing/appLocation.ts:13`) is in `SCREEN_INDEPENDENT_PARAMS` (`:35`) — the only param that
survives a screen change.

## Exploration 3 — RPC definition, server streaming, and the interactive-prompt precedent

### Proto-first, both sides generated

`.proto` is the single source of truth; there is no Rust-first codegen.

- **Declare here:** `packages/tddy-service/proto/connection.proto`, `service ConnectionService` (`:7`, ~100 methods).
- **Rust:** `packages/tddy-service/build.rs` — a `prost_build` pass installing `tddy_codegen::TddyServiceGenerator`,
  plus a second `tonic_build` pass with `.extern_path(".connection", "crate::proto::connection")` reusing the
  same structs. Regenerate: `cargo build -p tddy-service`.
- **TypeScript:** `packages/tddy-web/buf.gen.yaml` (`protoc-gen-es`, `out: src/gen`).
  Regenerate: `bun run --filter tddy-web generate` from the repo root.
- `packages/tddy-rpc-web/src/envelope-transport.ts` is the ConnectRPC `Transport` over LiveKit data
  channels, with `handleServerStreaming` (`:428`) and `handleBidiStreaming` (`:~495`).
- ⚠ `packages/tddy-rust-typescript-tests/gen/` is **stale and unused** (`docs/dev/TODO.md:1141`) — ignore.

### `StreamHostStats` is the exact precedent, end to end

- Declaration `connection.proto:255-262`; messages `:2245-2265`.
- Handler `packages/tddy-daemon/src/connection_service.rs:16529`; adapter `MpscHostStatsStream` `:863-882`
  (the generated trait needs `Stream<Item = Result<Out, Status>> + Send + Unpin`,
  `packages/tddy-codegen/src/generator.rs:93-106`); tonic adapter `connection_tonic_adapter.rs:1287-1305`.
- Generated TS `packages/tddy-web/src/gen/connection_pb.ts:8536-8548` (`methodKind: "server_streaming"`);
  the client method is an `AsyncIterable`. Note `uint64` → `bigint`.
- Hook `packages/tddy-web/src/rpc/useHostStats.ts:38`. **There is no shared streaming-hook helper** — the
  `useEffect` + `cancelled` flag + `for await` + swallow-AbortError shape is hand-copied per hook, and each
  new hook's doc comment cites `useHostStats` as the template. Siblings: `useSessionNotifications`,
  `useWorktreeStatsStream`, `useLiveKitRooms`, `useSessionWorktreeStats`.
- `useDaemonClient(Service)` returns `null` until a daemon is selected — **every call site must guard**.

### ⚠ Streaming teardown contract — bites the prompt stream specifically

`packages/tddy-codegen/docs/server-streaming.md`: the response pump must `break` on send error, **and a
handler whose stream can be silent must also `tokio::select!` on `tx.closed()`** — otherwise the handler task
leaks forever, one per subscription. `StreamHostStats` escapes this only because it emits every 5 s
unconditionally. Worked example: `pump_rooms` in `packages/tddy-daemon/src/livekit_rooms_stream.rs`; pinned
regression test `packages/tddy-daemon/tests/stream_livekit_rooms_rpc.rs::stops_reading_the_server_once_the_subscriber_is_gone`.

**A passphrase-prompt stream is silent almost all the time — it is exactly the case that leaks.** Any node
adding one owns that `tx.closed()` select and a teardown test.

### Testing: the in-memory backend fully supports streaming *and* bidi

- `anInMemoryRpcBackend()` — `packages/tddy-connectrpc-testkit/src/backend.ts:228`; class `:71`, a fluent
  builder over `createRouterTransport`. `implement`, `onUnary`, `failWith`, `transport()`, `callsTo`, `resetCalls`.
- `mountWithRpc(component, backend)` — `packages/tddy-web/cypress/support/rpc/inMemory.tsx:39`; also a Cypress
  command (`cypress/support/component.ts:43`).
- **Streaming needs no separate API**: `createRouterTransport` routes by `methodKind`, so a server-streaming
  method is implemented as an **async generator** and a bidi method as an async generator taking
  `AsyncIterable<Input>`.
- Repo-wide convention: a streaming fake ends with `await new Promise<never>(() => undefined)` to **stay open**
  — a completed stream reads to the UI as the daemon dropping the feed.
  Example `cypress/support/rpc/connectionServiceBackend.ts:566-586` (`streamHostStats`).
- ⚠ The recording interceptor **only records unary requests** (`backend.ts:147-157`, "Streaming messages are
  not captured in v1"), so streaming assertions use closure counters exposed on the backend
  (`connectionServiceBackend.ts:296-298`, `:637` — `hostStatsStreamCount()`).
- Daemon-scoped hooks need `withSelectedDaemon(...)` — `cypress/support/rpc/withSelectedDaemon.tsx:86`.
- Canonical streaming acceptance test to copy: `cypress/component/HostStatsFooterAcceptance.cy.tsx`
  (page object `cypress/support/pages/hostStatsFooterPage.ts`).
- Rust side: in-crate `#[tokio::test]` against the service struct, injecting a fake provider and short
  intervals — `connection_service.rs:18019-18118`, with a `next_event` timeout helper so a hang cannot pass.

→ **Every node in this stack is testable at both layers.** No node is blocked on test infrastructure.

### Server-asks-the-UI: exactly one first-class mechanism

`AcpService.Session` — `packages/tddy-service/proto/tddy/acp/v1/acp.proto:18-55`, a **bidi** stream with an
**application-level `id`** (JSON-RPC style) correlating a request with its reply, deliberately distinct from
`tddy_rpc`'s `(peer, request_id)`. The agent sends `AcpAgentMessage { id, request_permission }`; the client
replies `AcpClientMessage { id, request_permission: RequestPermissionResponse }`.

Web side: `useAcpSessionOverClient` — `packages/tddy-web/src/components/chat/useAcpSession.ts:129`. Outbound is
an `AsyncQueue<AcpClientMessage>`; the loop is `for await (const m of client.session(queue))` (`:311`); the
question arrives at `:381` (`case "requestPermission"` → `setPendingQuestion`) and is answered at `:479`
(`sendPermissionReply`). Test fakes: `cypress/support/rpc/acpSession.ts` (`acpQuestion`, `acpScriptedSession`,
`acpRecordingSession`); round-trip test `cypress/component/AgentChatAcpStreamingAcceptance.cy.tsx:75`.

Weaker, notification-only mechanisms (no reply channel): `SessionEntry.pending_elicitation` flag,
`StreamSessionNotifications` (`ATTENTION_REQUIRED`), the Codex OAuth participant-metadata relay.
The VNC/screen-sharing passphrase dialogs are the **opposite direction** — the UI decides to ask before
calling a unary; not server-initiated.

**Two viable shapes for the ssh-agent passphrase**, both with precedent in-repo:
1. A **bidi RPC** modelled on `AcpService.Session` with `id`-correlated envelopes.
2. A **server-stream + unary reply** pair (`StreamXxxPrompts` + `AnswerXxxPrompt(prompt_id, secret)`) — the
   `StreamHostStats` + `CalculateWorktreeSize` shape already used by `useWorktreeStatsStream`.

`MintLocalToken` (`connection_service.rs:~16481`) is the precedent for a **transport-restricted** method,
which matters for a method carrying a secret.

### ⚠ SSH passphrase prompting is actively suppressed today

`packages/tddy-core/src/worktree.rs:39-52` — `git_remote_command` hardens against interactive hangs:
`GIT_TERMINAL_PROMPT=0` and `.stdin(Stdio::null())`, "so a missing key/passphrase or credential prompt fails
fast instead of blocking forever — a headless daemon has no TTY to answer such a prompt."
`packages/tddy-daemon/src/config.rs:192-200` documents today's workaround: point `ssh_command` at an ssh binary
that authenticates non-interactively (macOS `/usr/bin/ssh -o BatchMode=yes` with Keychain).

→ The add-key node must **invert that hardening behind an explicit opt-in** and build the correlation channel.
This is the riskiest node in the stack and belongs near the top, after the plumbing it depends on is real.

## Exploration 4 — capability probes: what exists per requested column

| # | Requested column | Verdict |
|---|---|---|
| 1 | ssh-agent availability + loaded keys | **DOES NOT EXIST** |
| 2 | GitHub CLI status | **DOES NOT EXIST** in product code |
| 3 | git configured user | **DOES NOT EXIST** |
| 4 | VNC / RDP availability + connect | Bridges + viewer **EXIST**; availability probe and host scope **DO NOT** |
| 5 | disk / memory / CPU / load | Disk + per-core CPU **EXIST**; memory, load, CPU count, fan-out **DO NOT** |
| 6 | running a probe on a host | `Shell` tool + `run_capture_as_user` **EXIST**; a session-less host-probe RPC **DOES NOT** |

### 1. ssh-agent — nothing, and no crate for it

Exhaustive grep for `SSH_AUTH_SOCK` / `ssh-agent` / `ssh-add` finds three hits, none of them agent code: a
comment in `dev.daemon.yaml:118`, and a **test fixture** in `packages/tddy-supervisor/src/policy.rs:487` where
`("SSH_AUTH_SOCK", "/tmp/agent")` is the example of an env key the spawn policy **denies** (`resolve_env`,
`policy.rs:124`, is an allowlist and `SSH_AUTH_SOCK` is not on it — **relevant if a probe runs under the supervisor**).

`grep -n "name = .*ssh" Cargo.lock` → **zero matches**. No `ssh-agent-lib`, `ssh-key`, `russh`, `osshkeys`.

What does exist is unrelated:
- `packages/tddy-vm/src/library.rs:377-405` `generate_vm_ssh_keypair` — shells out to `ssh-keygen`; `qemu.rs:621-670`
  `ssh_opts`/`scp_opts` build argv for the system `ssh`/`scp` with `BatchMode=yes` and `IdentitiesOnly=yes`
  explicitly "so the ambient agent's keys cannot be tried" (`:629`). VM guest login only.
- `packages/tddy-core/src/worktree.rs:27-52` — `GIT_SSH_COMMAND` config string, `set_git_ssh_command`.
  Configured by `GitConfig { ssh_command }`, `packages/tddy-daemon/src/config.rs:191-201`.
- ⚠ `packages/tddy-remote-git-repo` is **not** an SSH client. It wears git's ssh-argv contract and speaks
  LiveKit; its "credentials" are daemon access/refresh tokens (`credentials.rs:18-30`), and its README (`:44`)
  says "No LiveKit credential of any kind." No ssh crate in its `Cargo.toml`.

→ **Open question: `ssh-add` subprocess vs an ssh-agent protocol crate.** The latter is a new external
  dependency and needs developer consent (CLAUDE.md § ASK).

### 2. `gh` CLI — absent from product code

`grep` for `gh auth` / `Command::new("gh")` / `gh api` over `packages/**` → **zero**. The only `gh` in the repo is
`scripts/ci-status.sh:37-84` (developer tooling).

`packages/tddy-github` is a **GitHub OAuth web-login** crate, not a `gh` wrapper: `GitHubOAuthProvider`
(`provider.rs:16-32`), `RealGitHubProvider`, `SessionTokenSigner` (HMAC tddy session tokens), and
`GitHubTokenStore { put, get }` (`token_store.rs:15-25`) retaining the raw GitHub access token per login
(`packages/tddy-daemon/src/github_token_store.rs:38-73`, owner-only perms). RPC surface `auth.proto:5-13`.
A **second, unrelated** model exists: `GITHUB_TOKEN`/`GH_TOKEN` env + `curl` REST in
`packages/tddy-workflow-recipes/src/github_rest_common.rs:19-34` and `packages/tddy-tools/src/github_pr.rs`.

→ "Is `gh` installed and authenticated, and as whom" is a genuinely new probe, unrelated to either.

### 3. git identity — absent, deliberately

Every `user.name` / `user.email` hit is a **test fixture writing** an identity into a temp repo. No
`git config --get` call site, no git-config abstraction. `DaemonConfig::git` holds only `ssh_command`.
`packages/tddy-daemon/src/session_room.rs:338` states commits are "Signed by the daemon under a fixed identity
rather than by whatever `user.email` the checkout" has — the repo **deliberately avoids** reading it today.

### 4. VNC / RDP — bridges and viewer exist, but per **session**, with no probe

- `packages/tddy-vnc` (RFB via the `vnc-rs` git dependency) and `packages/tddy-rdp` (IronRDP) are **outbound
  bridge binaries**: read a JSON `BridgeConfig` from **stdin** ("to avoid exposing credentials in argv/ps"),
  connect out to a remote desktop, republish the framebuffer as a **LiveKit video track**.
- Control plane `packages/tddy-service/proto/screen_sharing.proto:8-15` — `ScreenSharingService
  { ListTargets, AddTarget, RemoveTarget, UnlockVault, StartStream, StopStream }`, `enum Protocol { VNC, RDP }`.
  Impl `packages/tddy-daemon/src/screen_sharing_service.rs:64-72`; encrypted vault `screen_sharing_vault.rs`.
- Web viewer **exists**: `ScreenSharingOverlay.tsx` subscribes to the bridge participant's LiveKit `VideoTrack`.
  **There is no browser-side VNC/RDP protocol client** — no novnc/guacamole/rfb in `package.json`; rendering is
  always a LiveKit video track. Tabs are *removed* when the connection lacks `media` (`InspectorTabs.tsx:101-110`).
- ⚠ **Scoping is per-session**: every `screen_sharing.proto` request carries `session_token` + `session_id`, and
  the vault lives under the session dir.
- ⚠ **No availability probe of any kind** — no port probe, no `TcpStream::connect`, not even an `exists()` check
  on the resolved bridge binary. `resolve_vnc_binary_path` / `resolve_rdp_binary_path`
  (`config.rs:451-500`) are a **path guess**: config → sibling of `current_exe()` → bare name on `PATH`.
  A missing binary surfaces only as a spawn `error!` at `screen_sharing_service.rs:174`.

→ A host-scoped remote desktop means a **new host-scoped target model + vault scope**, reusing the existing
  bridge and overlay. Real work, but not a greenfield viewer.

### 5. Telemetry — half of it exists

`sysinfo = "0.33"` is a workspace dependency consumed by exactly one crate (`packages/tddy-daemon/Cargo.toml:88`).
No `procfs`, no `num_cpus`. `packages/tddy-daemon/src/host_stats.rs` (203 lines) is the whole implementation.

**Missing, confirmed by grep returning zero product hits** for `total_memory`, `available_memory`,
`refresh_memory`, `load_average`, `/proc/loadavg`, `/proc/meminfo`, `statvfs`:
- **memory** — `sysinfo` is never asked for it;
- **load average** — absent;
- **explicit CPU count** — only implicit as `per_core_percent.len()`;
- **all mounts** — `MountUsage` enumerates them internally but only the project dir's mount is returned;
- **per-host fan-out** — `useHostStats` is single-selected-daemon only.

### 6. Command execution and the multi-host architecture — complete, and it does the routing for us

- `Shell` exec tool: `packages/tddy-tool-engine/src/lib.rs:467-540` `tool_shell` → `sh -c`, 30 s default block,
  exposed as `ExecuteTool` / `StreamExecuteTool`. ⚠ **Every `ExecuteToolRequest` requires a `session_id`** —
  there is no session-less "run this on host X" RPC.
- `packages/tddy-daemon/src/spawner.rs:736-830` `run_capture_as_user(os_user, program, args) -> Result<String>` —
  `getpwnam_r` + `setgid`/`initgroups`/`setuid`, cwd `home`, null stdin, captures stdout, non-zero exit carries
  stderr. **This is the natural helper for a capability probe.**
- ✅ **Cross-host routing is already generic.** `connection_service.rs:9171-9183`
  `async fn rpc_served_by_peer<Req, Resp>(...)` relays a unary RPC to `daemon-{instance_id}` over the common
  room; `Ok(None)` means "mine to serve". `PeerRoute` was "renamed to reflect that this routing logic is now
  applied to all eligible RPCs" (`livekit_peer_discovery.rs:202-213`). **A new unary probe RPC on
  `ConnectionService` inherits cross-host routing for free** — no new transport work.
- ⚠ Trust model (`docs/ft/web/projects-screen-multi-host.md` § Trust model): "any participant that can join the
  LiveKit common room appears as an eligible daemon and can receive forwarded RPCs (including the caller's
  `session_token`). The common room is treated as a trusted peer group, **not a cryptographically authenticated
  one**." **This is load-bearing for the passphrase node.**
- ⚠ Note `ConnectionCapability` ("rpc"/"media"/"presence") means **what the wire carries**, not what software is
  installed on the host. The Hosts screen introduces a second, unrelated sense of "capability" — name it
  differently to avoid collision.

## Design decisions (interview round 2, 2026-09-06)

| Question | Decision |
|---|---|
| ssh-agent access | **Add an ssh-agent protocol crate.** Speak the agent wire protocol directly rather than shelling out to `ssh-add`, and decrypt the private key in-process with the supplied passphrase. This is **explicit developer consent for two new external dependencies** (an ssh-agent client crate + an ssh key crate) per CLAUDE.md § ASK. Avoids the `SSH_ASKPASS` / `SSH_ASKPASS_REQUIRE=force` helper-binary dance entirely. |
| Prompt channel shape | **Server-stream + unary reply.** `StreamHostPrompts` pushes the question; a unary `AnswerHostPrompt(prompt_id, ...)` returns it. Mirrors the `StreamHostStats` + `CalculateWorktreeSize` shape already used by `useWorktreeStatsStream`. The unary reply can be transport-restricted following the `MintLocalToken` precedent. **Not** the ACP bidi shape. |
| Passphrase confidentiality | **Explore client-side RSA encryption so the passphrase is end-to-end encrypted** — the browser encrypts under the target host's public key; only that host can decrypt. Supersedes the plain "forward, never persist" transport assumption; forward-and-drop still holds for the *plaintext* on the host. |
| Granularity | **8 nodes as proposed.** |

### The E2E-encryption idea: what it buys, and what it does not

**Feasible as stated.** A passphrase is small, so plain **RSA-OAEP** is sufficient — no hybrid AEAD envelope
needed. A 2048-bit key with OAEP-SHA-256 carries ~190 bytes of plaintext, comfortably more than any passphrase.
The browser side is `SubtleCrypto` (`importKey` + `encrypt` with `RSA-OAEP`) — **no new web dependency**. The
daemon side needs an RSA implementation, which is a **third** new Rust dependency to add to the two above.

⚠ **Honest limit on the guarantee, and it must be designed for in n6's PRD.** Encryption protects the
passphrase from a *passive* relay — the LiveKit common room and any daemon forwarding the call no longer see
plaintext. It does **not** by itself defend against an *active* peer, because the client learns the host's
public key over that same unauthenticated channel: a hostile common-room participant advertising itself as
`daemon-<instance_id>` can publish **its own** public key and decrypt what the browser sends. The trust model
doc is explicit that the common room is "a trusted peer group, not a cryptographically authenticated one."

So the encryption is worth doing, and n6's PRD must state which of these it commits to:
- **key continuity** — pin a host's public key on first sight and warn on change (SSH's own TOFU model); or
- **out-of-band verification** — show a key fingerprint the operator can compare against the host; or
- **accept the limit explicitly** — protect against passive observation only, and say so in the dialog.

### Consequence for node 6

`agent-add-key` now owns: the prompt stream + unary reply, the host keypair and its lifecycle, public-key
distribution, browser-side `SubtleCrypto` encryption, daemon-side decryption, and the agent key-add itself.
**This is the largest and riskiest node in the stack.** The encryption is not separable into a later node —
shipping a plaintext passphrase first and encrypting it afterwards would mean deliberately landing a weakness.
If its red phase shows it is too large to review, split it **by capability** (e.g. an unencrypted-but-IPC-only
add first, then the encrypted remote path) rather than by layer.


---

## Exploration 5 — node-specific: extending the telemetry surface

### The three extension points, exactly

**1. The trait** — `packages/tddy-daemon/src/host_stats.rs:57-62`:

```rust
pub trait HostStats: Send + Sync {
    fn cpu_per_core_percent(&self) -> Vec<f32>;
    fn disk_for_project_dir(&self) -> DiskUsage;
}
```

Adding a method is a **breaking change to every implementor**. Implementors found: `SysinfoHostStats`
(`:69`) in production, plus `FakeHostStats` and `SequencedHostStats` in
`connection_service.rs` tests (~`:18019-18118`). All are in-repo; no external implementor exists.

**2. The provider** — `SysinfoHostStats` (`host_stats.rs:69-137`) holds
`system: Mutex<sysinfo::System>`, long-lived so per-core deltas are real. It already calls
`system.refresh_cpu_usage()`. Memory needs `refresh_memory()` and then `total_memory()` /
`available_memory()`; `sysinfo` 0.33 exposes load average as an associated function, and core count
falls out of `system.cpus().len()`.

⚠ **Load average is not portable.** On Windows `sysinfo`'s load average returns zeros. The proto must
be able to say "this host does not report a load average" rather than reporting `0.0, 0.0, 0.0`, which
an operator would read as an idle machine. This is the CLAUDE.md no-fallbacks rule in concrete form.

**3. The wire** — `packages/tddy-service/proto/connection.proto:2250-2264`:

```proto
message HostStatsEvent {
  HostCpuStats cpu = 1;   // always populated
  HostDiskStats disk = 2; // always populated
}
message HostCpuStats { repeated float per_core_percent = 1; }
message HostDiskStats { uint64 available_bytes = 1; uint64 total_bytes = 2; string project_dir = 3; }
```

The documented contract (`:2250-2252`) is that **both blocks are always populated** with the latest
snapshot — a fast CPU tick re-sends the unchanged disk block. Any new block must either join that
always-populated contract or state its own rule explicitly at the declaration site.

### Cadence: which timer carries the new fields

`stream_host_stats` (`connection_service.rs:16529-16595`) runs two independent timers in a
`tokio::select!`: `cpu_tick` (default 5 s) and `disk_tick` (default 60 s), each pushing a **full**
event. Overrides: `with_host_stats_intervals(cpu, disk)` (`:2032`).

- **Memory** changes on the same timescale as CPU → the fast tick.
- **Load average** likewise → the fast tick.
- **Core count** is effectively static → include it in every event; it costs one `u32`.

Adding a third interval would mean a third timer and a third builder parameter for no user-visible
gain. Recorded as a decision, not left implicit.

### `uint64` → `bigint` in TypeScript

`connection_pb.ts` maps `uint64` to `bigint` (already true of `HostDiskStats.availableBytes`). Memory
byte counts will do the same, and `hostStatsFormat.ts:14` `formatDiskFree(availableBytes: number | bigint)`
already accepts both — a memory formatter should follow that signature rather than forcing a `Number()`
conversion at the call site.

### What already renders, and what a new field must reuse

`packages/tddy-web/src/components/sessions/`:
- `CpuCoresIndicator.tsx` — one mini bar per logical core.
- `DiskSpaceIndicator.tsx` — free-space readout.
- `hostStatsFormat.ts` — `formatDiskFree` (`:14`), `clampCorePercent` (`:23`).

A memory indicator is the same shape as the disk one (used/total with a human-readable free figure), so
it belongs beside them and should share the byte formatter rather than introducing a second one.

### Test doubles that must be updated in lockstep

Because the trait gains methods, the two in-test implementors must gain them too. Both are in
`packages/tddy-daemon/src/connection_service.rs`'s test module. `SequencedHostStats` advances a counter
per read to prove cadence — the new readings must participate in that sequencing or the cadence tests
become blind to them.
