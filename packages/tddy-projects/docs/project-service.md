# `project.ProjectService` (tddy-projects)

Five RPCs for logical projects: list and create, add an existing project on another host, list
branches on the project's main checkout, and set the default integration base ref. The proto is
`packages/tddy-service/proto/project.proto`; implementation modules are `project_storage.rs` and
`project_provision.rs` (extracted from `tddy-daemon` in `#unbundle` node 9).

## The surface

| RPC | Purpose |
|---|---|
| `ListProjects` | Projects known to this daemon for the caller |
| `CreateProject` | Register a new project and its checkout |
| `AddProjectToHost` | Reuse a `project_id` on another daemon instance |
| `ListProjectBranches` | Branches visible on the project's main checkout |
| `SetProjectDefaultBranch` | Set the integration base ref (forwarded for logical-project scope) |

## Transports

Registered on the same daemon transports as the other split services (HTTP `/rpc`, LiveKit rooms,
local socket).

Product docs: [project-concept.md](../../../docs/ft/daemon/project-concept.md),
[projects-screen-multi-host.md](../../../docs/ft/web/projects-screen-multi-host.md).
