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

`tests/` holds only the suites that exercise this composition — see
[test-placement.md](./test-placement.md).

## Local Unix socket

`local_socket_server.rs` is a **multi-service** tonic server: production clients such as
`tddy-sandbox-app` dial `session.SessionService` and `local_token.LocalTokenService` here, alongside
every other service the stack registered for local callers.

## Where subsystems live

| Coordinate | Crate / doc |
|---|---|
| `session.SessionService` | [tddy-session-lifecycle](../../tddy-session-lifecycle/docs/session-service.md) |
| `project.ProjectService` | [tddy-projects](../../tddy-projects/docs/project-service.md) |
| `demo_vm.DemoVmService` | [tddy-vm](../../tddy-vm/README.md) |
| `local_token.LocalTokenService` | [tddy-daemon-auth](../../tddy-daemon-auth/README.md) |
| `pr_stack.PrStackService` | [pr-stack-service.md](./pr-stack-service.md) |
| Host, worktree, catalog, exec-tool, … | Each crate's own service doc (see sibling packages) |

There is **no** `connection.ConnectionService` and no `connection.proto`.
