# Daemon endpoint (tddy-daemon)

After the `#unbundle` stack, **`tddy-daemon` is wiring only**: it loads configuration, assembles every
RPC service from its owning crate, serves them on HTTP `/rpc`, LiveKit, and the local Unix socket, and
implements **`daemon_config.DaemonConfigService`** for its own settings. It does not implement session
lifecycle, projects, tools, hosts, or any other subsystem RPC.

## What stays in this crate

Roughly twelve source modules under `src/`: `main`, `lib`, `server`, `startup`, `runtime`, `config`,
`daemon_settings`, `daemon_config_service`, `local_socket_server`, `user_sessions_path`,
`tddy_user_config`, `relay_idle`, plus PR-stack handlers in `pr_stack_rpc.rs` (family P stays on the
daemon by dependency design).

`runtime.rs` derives each `ServiceEntry` from configuration and returns handles; nothing that listens,
dials, or runs forever is started there.

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
