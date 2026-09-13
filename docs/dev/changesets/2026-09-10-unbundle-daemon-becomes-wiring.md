# 2026-09-10 — Dissolve ConnectionService — the daemon becomes wiring

**Type:** Architecture

Terminal node of the `#unbundle` stack ([#481](https://github.com/uppin/tddy-coder/pull/481), 9 of 9,
based on node 8's branch). The last **17** RPCs leave the deleted monolithic coordinate for four
services; **`connection.proto` and `ConnectionServiceImpl` are gone**. The stack's count goes from
**90 → 0** methods on `connection.ConnectionService` — the service no longer exists.

## What landed

| | |
|---|---|
| `tddy-session-lifecycle` *(new)* | Family C — 8 methods on `session.SessionService`; owns `TaskRegistry` |
| `tddy-projects` *(new)* | Family D — 5 methods on `project.ProjectService` |
| `tddy-vm` | Family O — `demo_vm.DemoVmService` (3 methods) |
| `tddy-daemon-auth` | Family Q — `local_token.LocalTokenService` (`MintLocalToken`; credential read stays in transport) |
| `tddy-daemon-sandbox` | Sandbox-IPC `HostRpcHandler` bridge; `self_arc` deleted |
| `tddy-daemon` | Endpoint only (~2,665 non-blank source lines); multi-service local socket |
| `tddy-service` | `connection.proto` **deleted**; `session`, `project`, `demo_vm`, `local_token` protos added |
| `tddy-web` / `tddy-coder` | Call sites and registration on real coordinates; Cypress connection fake removed |

Docs: [daemon-endpoint.md](../../../packages/tddy-daemon/docs/daemon-endpoint.md),
[session-service.md](../../../packages/tddy-session-lifecycle/docs/session-service.md),
[project-service.md](../../../packages/tddy-projects/docs/project-service.md).

## Deferred (documented, not in CI gate)

- **`tddy-desktop`** embeds `runtime::build` — verify on upgrade paths outside CI.
- **Stdio transport switch** in `docs/dev/todo/2026-07-01-tddy-daemon.md` — obstacle (`connection_service.rs`) removed; behaviour change still a follow-up.

## Verification

CI on `8e9e898f`: Rust **6669/6669**, web **2630/2630**; Rust lint, build, generated code green.
Scoped local gates during implementation: fmt, clippy on touched packages, `./test -p` for daemon,
session-lifecycle, projects, service.
