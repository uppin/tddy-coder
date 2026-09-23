# Daemon endpoint (tddy-daemon)

After the `#unbundle` stack, **`tddy-daemon` is wiring only**: it loads configuration, assembles every
RPC service from its owning crate, serves them on HTTP `/rpc`, LiveKit, and the local Unix socket, and
implements **`daemon_config.DaemonConfigService`** for its own settings. It does not implement session
lifecycle, projects, tools, hosts, or any other subsystem RPC.

## What stays in this crate

Eleven source modules under `src/`: `main`, `lib`, `server`, `startup`, `runtime`, `config`,
`daemon_settings`, `daemon_config_service`, `local_socket_server`, `index_daemon` (with
`index_daemon_body`). `src/lib.rs` declares those and nothing else — it carries no re-export facade,
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
