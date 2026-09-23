# Daemon endpoint (tddy-daemon)

After the `#unbundle` stack, **`tddy-daemon` is wiring only**: it loads configuration, assembles every
RPC service from its owning crate, serves them on HTTP `/rpc`, LiveKit, and the local Unix socket, and
implements **`daemon_config.DaemonConfigService`** for its own settings. It does not implement session
lifecycle, projects, tools, hosts, or any other subsystem RPC.

## What stays in this crate

Thirteen source modules under `src/`: `main`, `lib`, `server`, `startup`, `runtime`, `config`,
`daemon_settings`, `daemon_config_service`, `local_socket_server`, `agent_tool_socket`,
`common_room_key_directory`, `index_daemon` (with `index_daemon_body`). `src/lib.rs` declares those and nothing else — it carries no re-export facade,
and `config` is the single forwarding module, to `tddy-daemon-kernel`, which owns the configuration
this crate loads.

`runtime.rs` derives each `ServiceEntry` from configuration and returns handles; nothing that listens,
dials, or runs forever is started there.

### Assembling the session host and the RPC families

`runtime::build` builds the `DaemonSessionHost` with every `with_*` first, then calls
`tddy_daemon_rpc::RpcHandlers::install(host)`, which builds the Project, Catalog, ExecTool and
PR-stack handlers from the host's state and installs them on it as its `DaemonRpcFamilies` port.
**That install is the last step before the host goes behind its `Arc`**: the handlers share the
host's `Arc`s, so a `with_*` applied afterwards would leave them holding the value it replaced (the
lifecycle crate `debug_assert`s this). The four families' local-socket services
(`rpc_handlers.project_service()`, `catalog_service()`, `exec_tool_service()`,
`pr_stack_service()`) and their transport entries (`rpc_handlers.entries()`) come from the returned
handlers, and `BinaryLocalSocketServices` names `ProjectServiceImpl<ProjectRpcHandler>`,
`CatalogServiceImpl<CatalogRpcHandler>`, `ExecToolServiceImpl<ExecToolRpcHandler>` and
`PrStackServiceImpl<PrStackRpcHandler>`. See
[tddy-daemon-rpc](../../tddy-daemon-rpc/docs/architecture.md).

`tests/` holds only the suites that exercise this composition — see
[test-placement.md](./test-placement.md).

## The signing identity

`runtime::build` owns the daemon's one signing identity. In order:

1. The common room, if any, is resolved **once** (`CommonRoomTarget::from_livekit`) together with
   its `CommonRoomPeerRegistry`, before auth — the roster is also where peers' session-token keys
   are read from, so the key directory and peer discovery cannot disagree about whether there is a
   room.
2. With a `github:` block, `tddy_daemon_auth::load_signing_key` loads — or on first boot generates —
   the `DaemonSigningKey`, against a config whose `tddy_data_dir` the runtime has already pinned.
   With no `github:` block nothing signs or verifies, and no key is loaded.
3. The key directory is `CommonRoomKeyDirectory` over that registry when there is a room, else
   `StandaloneKeyDirectory`. One `SessionTokens` is built from key + directory and passed to
   `build_auth_entries_with`; the local socket, `local_token.LocalTokenService` and the session
   host (`DaemonSessionHost::with_session_tokens`) all take their signer from
   `AuthBuildResult::session_tokens`, so every token this daemon mints carries the key it
   advertises.
4. `advertised_signing_key(&key)` becomes the discovery loop's `AdvertisedSigningKey`, published on
   every (re)connection to the common room.

`tests/runtime_signing_identity_acceptance.rs` pins that the key `runtime::build` advertises is the
key it signs with.

### `common_room_key_directory` — the fleet's `KeyDirectory`

The adapter between `tddy_daemon_auth::KeyDirectory` (an auth-owned trait) and the opaque
`AdvertisedSigningKey` strings `tddy-daemon-livekit` carries. It can live in no other crate:
`tddy-daemon-livekit`'s `dependency_boundary_unit` forbids it from reaching auth, and this is the
crate that depends on both — which is why `unbundle_endpoint`'s closed module list admits it, and
why this crate depends on `ed25519-dalek` directly (to decode SPKI).

- **Resolution.** `public_key_for(kid)` asks the registry for **every** candidate advertised under
  that id and keeps the one whose decoded SPKI hashes to it (`KeyId::of(key) == kid`). A participant
  re-advertising a genuine id with other bytes therefore cannot shadow the real key, and a
  malformed or mismatched advertisement is refused, never cached.
- **Learned keys are remembered across `CommonRoomPeerRegistry::clear`**, which discovery runs
  every time its room connection ends. Safe, because an id is a digest of its key: a remembered
  answer can never become a wrong one. Without it every peer token — a 24 h split-agent credential
  included — would be refused while this daemon reconnects. The cache grows by one entry per peer
  identity ever verified.
- **The trade is revocation.** A learned key is never evicted for the life of the process, so a
  peer that left the room, a removed host, or a compromised key keeps having new tokens accepted
  until this daemon restarts. There is no expiry and no revocation list.
- **It never waits.** Every answer comes from memory — the registry snapshot or the learned cache —
  which is what `DirectorySessionTokenVerifier::verify_now`'s single poll requires of a directory.
- **Only participants the identity rule admits are in the registry** — see
  [`tddy-daemon-livekit`](../../tddy-daemon-livekit/docs/livekit-service.md) § Signing keys.

## Local Unix socket

`local_socket_server.rs` is a **multi-service** tonic server: production clients such as
`tddy-sandbox-app` dial `session.SessionService` and `local_token.LocalTokenService` here, alongside
every other service the stack registered for local callers.

`tests/local_socket_family_wiring_acceptance.rs` is the guard on what `runtime.rs` actually hands
the socket. It builds the binary runtime with `runtime::build(…, RuntimeOptions::for_binary())`,
starts it, dials the Unix socket it assembled in a temp dir, and calls one method of each of the
session, Project, Catalog, ExecTool and PR-stack families, and asserts that each is answered by its
handler rather than by the transport's `Unimplemented`. The token-bearing calls carry a token no
daemon issued, so the handler's own first check is what answers, without depending on any session
or project existing. `local_socket_reachability_acceptance.rs` reads `local_socket_server.rs` as text,
and `local_token_uds.rs` mounts services it builds for itself; neither sees `runtime.rs`'s wiring.

## Where subsystems live

| Coordinate | Crate / doc |
|---|---|
| `session.SessionService` | [tddy-session-lifecycle](../../tddy-session-lifecycle/docs/session-service.md) |
| `project.ProjectService` | [tddy-projects](../../tddy-projects/docs/project-service.md) (trait, adapter); handler [tddy-daemon-rpc](../../tddy-daemon-rpc/docs/architecture.md) |
| `catalog.CatalogService` | [tddy-discovery](../../tddy-discovery/docs/catalog-service.md) (trait, adapter); handler [tddy-daemon-rpc](../../tddy-daemon-rpc/docs/architecture.md) |
| `exec_tools.ExecToolService` | [tddy-tool-engine](../../tddy-tool-engine/README.md) (trait, adapter); handler [tddy-daemon-rpc](../../tddy-daemon-rpc/docs/architecture.md) |
| `demo_vm.DemoVmService` | [tddy-vm](../../tddy-vm/README.md) |
| `local_token.LocalTokenService` | [tddy-daemon-auth](../../tddy-daemon-auth/README.md) |
| `pr_stack.PrStackService` | [pr-stack-service.md](./pr-stack-service.md) |
| Host, worktree, … | Each crate's own service doc (see sibling packages) |

There is **no** `connection.ConnectionService` and no `connection.proto`.
