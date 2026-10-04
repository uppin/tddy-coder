# `project.ProjectService` (tddy-projects)

Six RPCs for logical projects: list and create, add an existing project on another host, list
branches on the project's main checkout, set the default integration base ref, and set the accounts
the project uses. The proto is
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
| `SetProjectAccounts` | Replace the project's account assignments (forwarded for logical-project scope) |

The handlers that serve these RPCs live in
[`tddy-daemon-rpc`](../../tddy-daemon-rpc/docs/architecture.md) (`src/project/`); this crate owns the
trait they implement, the storage and the provisioning.

## Account assignments

`ProjectData.accounts` is a list of `AccountAssignment { provider, account_id }` pairs: the accounts
this project uses, as minted by the credential vault on the host that stored them. An `account_id` is
unique only within its provider, so an assignment names both halves. The field is
`#[serde(default, skip_serializing_if = "Vec::is_empty")]`: a row with no assignment writes no
`accounts` key, a row without the key reads as empty, and a daemon that predates the field reads a
row that carries it. An empty list means no account is assigned; every operation that needs one
fails with that as its stated reason.

`SetProjectAccounts(session_token, project_id, accounts, daemon_instance_id)` **replaces the whole
list** (`project_storage::set_project_accounts`). It never merges, so two concurrent edits cannot
interleave into a set neither person chose, and an empty list returns the project to unassigned.
A project uses **one account per provider**: a list naming the same provider twice is refused with
`InvalidArgument`, and the message names the provider. The handler enforces the refusal for the
status code and storage enforces it as the invariant. An unknown `project_id` is an error.

The scope is the logical project, exactly as for `SetProjectDefaultBranch`: when
`daemon_instance_id` addresses a peer, or other hosts own the same `project_id`, the call is
forwarded to them (`forward_set_project_accounts_via_livekit` in
[`tddy-daemon-livekit`](../../tddy-daemon-livekit/docs/livekit-service.md)). An assignment forwarded
to a peer names an account that peer's vault may not hold; the resolver reports that state as
`UnknownOnThisHost`, not as unassigned.

`ProjectEntry.accounts` carries each project's assignments on every response that returns a
project, so `ListProjects` renders them without a second round trip.

This crate **stores** the assignment and does not depend on `tddy-credentials`. Interpreting it
against a vault is
[`resolve_account`](../../tddy-accounts/docs/accounts-service.md#resolving-a-projects-account) in
`tddy-accounts`.

## Transports

Registered on the same daemon transports as the other split services (HTTP `/rpc`, LiveKit rooms,
local socket).

Product docs: [project-concept.md](../../../docs/ft/daemon/project-concept.md),
[projects-screen-multi-host.md](../../../docs/ft/web/projects-screen-multi-host.md).
