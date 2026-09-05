# Initial discovery — optional-livekit node `session-connection`

Stack: `optional-livekit`. Exploration 1 is the whole-stack discovery pass, reproduced in full so this
node's companion stands alone once the shared source is deleted at the wave-1 checkpoint.

---

## Exploration 1 — whole-stack discovery

**Status:** in progress (temporary; copied into each node's `{slug}-initial-discovery.md` at Step 4b, then deleted)
**Stack work name:** optional-livekit
**Date:** 2026-09-05

## Goal (from the request)

Isolate LiveKit in `tddy-web` behind transport abstractions so it becomes completely optional.
`tddy-desktop` registers its own daemon-host override with IPC-based RPC connection factories;
LiveKit remains available to desktop when configured, and other hosts reach the desktop host over
LiveKit. IPC must be multi-connection (session-bound connections created the same way a LiveKit
room/participant is created for a session). Nothing LiveKit-shaped (rooms, participants) may appear
in the IPC path.

## Combined conclusions

_(filled in as explorations land)_

### Exploration 1a — where LiveKit is wired into tddy-web, and what desktop already has

### Scale

- `packages/tddy-web/src` — **30 files import `livekit-client`**, 8 import `tddy-livekit-web`;
  ~200 files under `src` + `cypress` mention LiveKit at all.
- `packages/tddy-desktop` (Tauri) — `src-tauri/src/{lib,ipc,config_source,oauth_callback}.rs`.
- `packages/tddy-tauri-web` — the browser half of the IPC transport (`transport.ts`).
- `packages/tddy-tauri-rpc` — the host half (`host.rs`, `WebviewRpcHost`).

### What already exists (State A)

**1. Serving-daemon transport is already host-agnostic.**
`src/rpc/daemonTransport.ts` picks a flavour at runtime via `daemonTransportFlavour(window)`
(`__TAURI_INTERNALS__` present => `webview-ipc`, else same-origin `/rpc`). Both flavours carry the
same interceptor stack (traffic meter + auth gate). `createDefaultDaemonTransport()` is the seam.
So *the daemon that serves the page* already works over IPC in desktop — this part is done.

**2. Cross-host selection is hard-wired to LiveKit.**
`src/rpc/selectedDaemon.tsx` (335 lines) owns:
- `SelectedDaemonProvider` -> `useCommonRoomDaemons` -> `useCommonRoom(livekitUrl, commonRoom, identity)`
  -> a `livekit-client` `Room`;
- the daemon list derived from `useRoomParticipants(room)` + `daemonHostsFromParticipants`
  (`src/lib/participantRole.ts`, parses a JSON advertisement out of participant metadata);
- `useDaemonClientFor(service, instanceId)` = `useLiveKitClient(service, room, daemonRpcIdentity(instanceId))`;
- `SelectedDaemonContextValue` **exposes `room: Room | null`** to the whole subtree.
The context type itself is LiveKit-shaped, so every consumer of `useSelectedDaemon().room` is coupled.

**3. Transport seam exists but names LiveKit.**
`src/rpc/transportProvider.tsx` (336 lines) provides `httpTransport` (serving daemon) plus
`liveKitFactory: (room: Room, targetIdentity: string, opts) => Transport`, and the hooks
`useLiveKitTransport`, `useLiveKitClient`, `useLiveKitTransportFactory`,
`useLiveKitTransportFactoryIsOverridden`. `Room` + `targetIdentity` are in the signature, so the
seam cannot express a non-LiveKit peer connection today.

**4. Session-bound connections are LiveKit rooms.**
- `useSessionAttachment.ts` — `ConnectSession`/`ResumeSession` reply carries
  `{livekitRoom, livekitUrl, livekitServerIdentity}`; a non-empty `livekitRoom` yields
  `status: "connected-livekit"`, otherwise `status: "connected-grpc"`.
- `useSessionLiveKitRoom.ts` — joins a **second** LiveKit room per attached session, minting a
  `web-traffic-*` observer identity.
- `sessionParticipantRpcClient.ts` — targets `daemon-<instanceId>-<sessionId>`, the session process's
  own LiveKit participant identity (daemon side: `spawner.rs::livekit_server_identity_for_session`,
  `split_session.rs::split_agent_participant_identity`).
- `useLiveKitTerminalToken.ts` / `TokenService.generateToken|refreshToken` — mints a per-room browser
  token, refreshed on a TTL timer.
- `sessionClientCache.ts` — caches session clients keyed by `targetIdentity` + the `Room` object
  identity as `transportKey`.

**5. A degraded non-LiveKit session path already exists.**
`connected-grpc` is a real status handled in `SessionRuntime.tsx`, `SessionMainPane.tsx`,
`SessionDetailPane.tsx`, `SessionsDrawerScreen.tsx`, `sessionRuntimeRegistry.ts`. On that path
session-scoped RPC falls back to the **daemon client** (`SessionRuntime.tsx:176`) and the terminal
uses `GhosttyTerminalGrpc` instead of `GhosttyTerminalLiveKit`. It is explicitly degraded — a
`TODO` at `SessionRuntime.tsx:264` notes gRPC sessions do not plumb everything, and the
connection-handshake overlay is LiveKit-only (`SessionRuntime.tsx:130`, `:545`).

**6. Genuinely LiveKit-specific (media) surfaces.**
`ScreenSharingOverlay`, `SessionScreenSharingTab`, `VncOverlay`, `SessionVncTab`,
`ParticipantVideoPreviewDialog`, `hooks/participantCameraVideo.ts`, `useRoomParticipants`,
`ParticipantList`, `LiveKitRoomsPanel`, `LiveKitAppPage` — these consume LiveKit **tracks** and
presence, not just RPC. They cannot be expressed over a plain frame pipe without a media plan.

**7. The IPC bridge is single-connection today.**
- `tddy-tauri-web/src/transport.ts` — `createTauriIpcBridge()` registers **one** `Channel` per page
  via `invoke("tddy_rpc_connect", {channel, clientEpoch})`; `send` is `invoke("tddy_rpc_send", frame)`.
- `daemonTransport.ts` keeps a module-level `thisPagesBridge` singleton, deliberately: "registering a
  response channel *abandons the previous one*".
- `tddy-tauri-rpc/src/host.rs` — `WebviewRpcHost` "Hosts `S` for a **single webview at a time**";
  `connect()` abandons the previous `Connection`; frames from a replaced epoch are refused with
  `FrameError::StaleConnection`.
- `tddy-desktop/src-tauri/src/ipc.rs` — the two commands `tddy_rpc_connect` / `tddy_rpc_send`, one
  `RpcState` holding one `WebviewRpcHost<MultiRpcService>`.
So "IPC should be multi-connection" is a **real change in `tddy-tauri-rpc` + `tddy-tauri-web` +
`tddy-desktop`**, not only a tddy-web abstraction: today one page = one connection = the daemon
roster, with no addressing for a session peer.

### Combined conclusions

- The work splits along four seams: **(a)** a host/peer *directory* (who can I talk to), **(b)** a
  host-level *connection factory* (how do I get a transport to host H), **(c)** a *session-bound*
  connection (how do I get a transport to session S on host H, plus its lifecycle/status), and
  **(d)** *media* capabilities (tracks), which are LiveKit-only and must degrade, not abstract away.
- `RpcTransportProvider` and `SelectedDaemonProvider` are the two existing DI seams; both already
  have test-injection overrides, so the abstraction can be introduced without inventing new plumbing
  — but both currently spell `Room`/`targetIdentity` in their types.
- A `connected-grpc` path already exists end-to-end, which means the *shape* of "a session without
  LiveKit" is proven; what is missing is a first-class, multi-connection, capability-aware version of
  it plus a desktop-registered IPC host override.
- Blast radius on tests is very large (~120 Cypress component specs mention LiveKit), so the stack
  must keep the LiveKit path working unchanged at every step.

### Exploration 1b — sizing the seams, and the constraints the stack must hold

### Consumers of the LiveKit `Room` exposed by `SelectedDaemonProvider`

`useSelectedDaemon()` is called in 14 files; only **8** read `.room`, and they split cleanly in two:

| Purpose | Files | Which node owns the migration |
|---|---|---|
| Build a **peer RPC client** | `rpc/useHostFanOut.ts:108`, `components/models/useModelRegistryFanOut.ts:207`, `components/models/ModelChatDialog.tsx:38`, `components/projects/ProjectsAppPage.tsx:104`, `components/sessions/SessionsDrawerScreen.tsx:93,102-109`, `rpc/selectedDaemon.tsx:323` | n1 — the connection model |
| Read **presence** (`useRoomParticipants`) | `components/livekit/LiveKitAppPage.tsx:21-22`, `rpc-playground/RpcPlaygroundAppPage.tsx:81-82`, `components/sessions/SessionsDrawerScreen.tsx:315-317,856` | n2 (accessor) / n4 (gating) |

That split is what lets n1 and n2 be separate nodes without fighting over the same lines: n1 removes
every **RPC** use of `.room`, so by n2 only presence consumers remain and `room` can leave the
context's public shape.

`SessionsDrawerScreen.tsx` uses `room` for three different things (cross-host daemon client at
:102-109, session-client fallback at :257-284, presence at :315-317) — the one file where node
boundaries need care.

### Terminal components

`GhosttyTerminalLiveKit.tsx` is **736 lines**; `GhosttyTerminalGrpc.tsx` is **631**;
`SessionLiveKitTerminal.tsx` is 95. Merging the two into one connection-fed terminal is a node of its
own (n5) rather than part of the session-connection node (n3) — n3 already rewrites the attachment
status across `SessionRuntime`, `sessionRuntimeRegistry`, `SessionMainPane`, `SessionDetailPane` and
`SessionsDrawerScreen`.

### Daemon-side facts that shape the IPC addressing

- The **daemon serves one roster** over `/rpc` (`server.rs::run_server` -> `MultiRpcService`), and
  the same roster is what `WebviewRpcHost` hosts in desktop (`ipc.rs::RpcState`).
- A **session process** (`tddy-coder`) joins LiveKit under its own identity
  (`spawner.rs::livekit_server_identity_for_session` -> `daemon-{instance}-{session}`;
  split agents via `split_session.rs::split_agent_participant_identity`).
- But the daemon **can already serve session terminal RPC itself**:
  `cli_session_manager.rs:1052` hosts `terminal.TerminalService/StreamTerminalIO` bridged to a PTY
  handle. That is what the existing `connected-grpc` path rides on
  (`SessionRuntime.tsx:176` falls back to the daemon client).
- So a session-bound IPC connection is an **addressing** problem, not a new transport problem: the
  webview host needs to route a frame to a named peer (`daemon-<instance>` or
  `daemon-<instance>-<session>`) instead of to its single roster.

### Constraints

- **No public npm registry.** Dependencies resolve locally: `bun run resolve-local-lock` +
  `scripts/local-bun-install.sh` (`bun run local-registry-install`), against
  `LOCAL_REGISTRY_URL` (default `https://npm.dev.wixpress.com`). Plain `bun install` against the
  public registry is not available. **The stack therefore adds zero new npm dependencies** — every
  abstraction lives in existing workspace packages (`packages/tddy-web`, `packages/tddy-tauri-web`,
  `packages/tddy-rpc-web`).
- Root `package.json` workspaces: `tddy-web`, `tddy-livekit-web`, `tddy-rpc-web`, `tddy-tauri-web`,
  `tddy-desktop`, `tddy-rust-typescript-tests`, `tddy-connectrpc-testkit`.
- CI does **not** cover `tddy-desktop` or Cypress e2e (see `docs/dev/guides/ci.md`), so nodes 6 and 7
  need local verification evidence beyond a green PR.

### The agreed decomposition (7 nodes)

| Node | id | parents | Owns |
|---|---|---|---|
| 1 | `connection-model` | — | `src/rpc/connections/*`, provider registry, LiveKit provider, every daemon-level RPC call site |
| 2 | `host-directory` | n1 | `HostDirectory`/`HostDescriptor`/`HostDirectorySource`, LiveKit source, `room` leaves the context |
| 3 | `session-connection` | n1 | `openSession` -> `SessionConnection`; unifies `connected-livekit` + `connected-grpc` |
| 4 | `capability-gating` | n2, n3 | media/presence surfaces gated on connection capability |
| 5 | `terminal-convergence` | n4 | one terminal fed by the session connection |
| 6 | `multi-connection-ipc` | — | `WebviewRpcHost` concurrent addressed connections; `ipc.rs` target; `tddy-tauri-web` `openConnection` |
| 7 | `desktop-ipc-host` | n4, n6 | desktop registers the IPC provider for its own host; LiveKit opt-in for peers |

n1 and n6 are roots and start in parallel; n2 and n3 are parallel once n1 lands.

## Exploration 2 — node-specific: every place the two session statuses are branched on

### The branch points to remove

| File:line | Branch |
|---|---|
| `useSessionAttachment.ts:60-73` | `attachmentStateFromResponse` — `resp.livekitRoom !== ""` decides the status |
| `useSessionAttachment.ts:10-22` | the `SessionAttachmentState` union itself |
| `sessionRuntimeRegistry.ts:21` | `type SessionRuntimeStatus = "connected-livekit" \| "connected-grpc"` |
| `SessionRuntime.tsx:130` | handshake overlay is `connected-livekit` only |
| `SessionRuntime.tsx:176` | `if (runtime.status === "connected-grpc") return client ?? null` — the daemon-client fallback |
| `SessionRuntime.tsx:264` | `TODO`: gRPC sessions don't plumb everything |
| `SessionRuntime.tsx:428` | `connected-grpc` renders `GhosttyTerminalGrpc` |
| `SessionRuntime.tsx:545` | overlay comment — "LiveKit-only" |
| `SessionRuntime.tsx:639-643` | attachment -> runtime status mapping |
| `SessionMainPane.tsx:145` | `isConnected` = either status |
| `SessionDetailPane.tsx:22,46` | `isConnected` = either; `:46` renders a `connected-grpc` notice |
| `SessionsDrawerScreen.tsx:230,233` | `isConnected` = either |
| `SessionsDrawerScreen.tsx:388,399` | restore path fabricates a `connected-grpc` state with empty LiveKit fields |
| `SessionsDrawerScreen.tsx:530,533` | registry restore, same shape |

`SessionsDrawerScreen.tsx:399` is worth calling out: it constructs
`{status: "connected-grpc", livekitUrl: "", livekitRoom: "", livekitServerIdentity: "", identity: ""}`
— a state carrying four empty LiveKit fields purely to satisfy the union. That disappears entirely
once the status is one.

### The session-room join, and why it is a *second* room

`useSessionLiveKitRoom` calls `useCommonRoom(attachment.livekitUrl, attachment.livekitRoom,
identityRef.current)` with a freshly minted `web-traffic-<random>` identity, regenerated only when
the room name changes. So an attached session holds **two** LiveKit rooms open: the common room from
`SelectedDaemonProvider` and this one. The daemon names the session room via
`spawner.rs::resolve_livekit_room_name` — the configured common room when set, else
`daemon-{session_id}` — so in a common-room deployment both joins are to the *same* room under two
identities.

That is exactly the kind of detail a `SessionConnection` should absorb: no consumer needs to know
whether opening a session costs a new room or reuses one.

### The client cache contract to preserve

`SessionClientCache.clientFor(targetIdentity, transportKey, create)` compares `transportKey` **by
reference** and exists because `SessionsDrawerScreen` builds a session client inline while rendering.
Its documented guarantee: *the same instance while the target and its transport hold, a fresh one as
soon as either genuinely changes* — because `useAcpReplay` keys an effect on the client and would
otherwise cancel an in-flight snapshot pull on every host render. The `SessionConnection` becomes the
`transportKey`; the guarantee must not weaken.

### The `onRoom` capture

`SessionsDrawerScreen.tsx:257-284` captures the session's connected `Room` from the terminal's
`onRoom` callback, because the terminal is what actually calls `room.connect`, and falls back to the
common room when `liveKitFactoryIsOverridden` (a test double). A `SessionConnection` that owns its
own join removes the need for the callback — the connection knows its transport before any terminal
mounts. Worth confirming no other consumer depends on `onRoom` firing.

### Where the daemon serves a session without LiveKit

`packages/tddy-daemon/src/cli_session_manager.rs:1052` hosts
`terminal.TerminalService/StreamTerminalIO` bridged to a PTY handle, on the daemon's own roster. That
is what `connected-grpc` rides on, and it is why a `{"rpc"}`-only session connection is a real,
already-working configuration rather than a hypothetical.
