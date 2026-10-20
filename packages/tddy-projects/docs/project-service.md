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
forwarded to them (`forward_set_project_accounts` in
[`tddy_daemon_kernel::peer_calls`](../../tddy-daemon-kernel/docs/daemon-kernel.md#peer_calls--typed-rpcs-forwarded-to-a-peer)). An assignment forwarded
to a peer names an account that peer's vault may not hold; the resolver reports that state as
`UnknownOnThisHost`, not as unassigned.

### Local gh

`ProjectData.local_gh: bool` marks a project whose sessions take their GitHub identity from the
host's own `gh` login instead of a vault account. It is `#[serde(default, skip_serializing_if =
"is_false")]`: a project not using it writes no `local_gh` key, and a row without the key reads as
not using it, so rows written before the field existed read unchanged.

`SetProjectAccounts` carries the flag beside the list (`SetProjectAccountsRequest.local_gh`), and
`project_storage::set_project_accounts(dir, id, accounts, local_gh)` writes both in one replace. The
two sources can never both be in effect: a list that assigns a GitHub account together with
`local_gh` is refused with `InvalidArgument`, the message naming both and what to do ("a project that
uses local gh is assigned no github account; turn local gh off to assign one"). A refused call
leaves the row exactly as it was. Other providers' assignments are kept as sent. There is no
separate RPC for the toggle. `ProjectEntry.local_gh` (field 9) returns it on every response that
returns a project.

Like the accounts, the flag is stored on the host the request names. The assignment does not reach
the other hosts owning the project; see
[the open TODO](../../../docs/dev/todo/2026-10-14-project-account-assignment-reaches-one-host.md).
A session start forwarded to another host carries the source instead
([session identity](../../tddy-session-lifecycle/docs/session-identity.md#which-source-a-session-uses)).

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
