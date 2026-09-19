# PRD: Project → account assignments

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Stack**: `#keyring` 5/9 · branch `feature/keyring/assignments` · base `feature/keyring/accounts` (#511)

## Affected Features

- [Project concept](../project-concept.md) — a project row gains assigned account identifiers
- [Projects screen — multi-host](../../web/projects-screen-multi-host.md) — where the assignment is made
- [Git integration base ref](../../coder/git-integration-base-ref.md) — the precedent this follows exactly
- [Cross-daemon session authentication](../session-auth.md)

## Summary

Gives a project row a list of assigned account identifiers, an RPC to set them, a control on the
Projects screen to choose them, and a **resolver** that answers "which account does this project use
for this provider" — with **no fallback** when the answer is "none".

Project A uses GitHub accounts A and B; project C uses account B. A project with no GitHub account
assigned resolves to nothing, and the operations that need one will say so rather than guessing.

## Background

### The registry row

`packages/tddy-projects/src/project_storage.rs:10` — `ProjectData` is the row in
`~/.tddy/projects/projects.yaml`: `project_id`, `name`, `git_url`, `main_repo_path`, and three
optional fields added over time, each `#[serde(default, skip_serializing_if = …)]`. `ProjectsFile`
is `{ projects: Vec<ProjectData> }`, read with `read_projects` (missing file → empty vec) and written
whole with `write_projects` through `write_atomic`.

Adding a field is therefore a solved problem here, and `main_branch_ref` is the precedent for all of
it: an optional stored value, a documented behaviour when absent, and a `SetProjectDefaultBranch` RPC
described in `project.proto:22-24` as *"logical-project scope: forwarded to peer hosts owning the
same project_id"*.

### What forces a choice

An `AccountId` is **daemon-minted** (`#keyring` 3/9) and lives in that daemon's own vault. So an
assignment forwarded to a peer host names an account that peer may not have — and will not have
until `#keyring` 6/9 propagates the vault.

That is a real state, not an edge case, and it must have a name. Reporting it as "not assigned"
would be wrong: the assignment exists, and the person who made it would see their own choice absent
on another host with no explanation.

## Proposed Changes

### The row gains one field

```rust
/// Account identifiers this project uses, as minted by the vault on this host
/// (`#keyring` 3/9). Empty or absent = no account is assigned, and every operation
/// that needs one fails with that as its stated reason.
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub accounts: Vec<String>,
```

A **list**, because the requirement is explicitly several accounts per project. A project may hold at
most one account **per provider**; two GitHub accounts on one project is refused at the RPC with that
as the reason, because "which one" would otherwise be answered by ordering.

`ProjectEntry` in `project.proto` gains `repeated string accounts = 8;` so `ListProjects` carries the
assignment and no second round trip is needed to render it.

### `SetProjectAccounts`

```proto
rpc SetProjectAccounts(SetProjectAccountsRequest) returns (SetProjectAccountsResponse);

message SetProjectAccountsRequest {
  string session_token = 1;
  string project_id = 2;
  repeated string accounts = 3;        // replaces the whole list
  string daemon_instance_id = 4;
}
```

Shaped exactly like `SetProjectDefaultBranch`, including the logical-project scope — forwarded to
peer hosts owning the same `project_id`. Replace-the-whole-list rather than add/remove, so two
concurrent edits cannot interleave into a set neither person chose.

### The resolver, and its four answers

```rust
pub enum AccountResolution {
    Assigned(AccountId),
    NotAssigned,
    UnknownOnThisHost(AccountId),
    Ambiguous { provider: ProviderId, count: usize },
}
```

| Answer | When | Why it is not collapsed into another |
|---|---|---|
| `Assigned` | exactly one account for that provider, present in this host's vault | — |
| `NotAssigned` | no account for that provider | the person never chose one |
| `UnknownOnThisHost` | assigned, but this host's vault has no such record | the person **did** choose; this host cannot see it yet (until 6/9) |
| `Ambiguous` | more than one for the provider | should be unreachable — the RPC refuses it — so reaching it is a bug to report, not to resolve |

⚠ **`NotAssigned` resolves to nothing, and nothing is a valid answer.** No falling back to the
caller's own login, to "the only account in the vault", or to an unauthenticated request. The
developer's instruction is explicit — *"if github account is not assigned, the backend wouldn't be
able to resolve it to use those credentials"* — and a fallback here would silently push commits under
the wrong identity, which is the exact failure the whole stack exists to prevent.

### Where the resolver lives

In `tddy-accounts` (`#keyring` 4/9), not in `tddy-projects`. It needs the vault to answer
`UnknownOnThisHost`, and making `tddy-projects` — a crate about repositories, branches and checkout
paths — depend on the credential store to answer a question about assignment would put a dependency
on every project consumer for something most never use.

`tddy-projects` owns **storing** the assignment; `tddy-accounts` owns **interpreting** it.

### Projects screen

Each project row gains an account control per provider: the assigned account's label, or *"Not
assigned"*, or *"Assigned, not available on this host"* — the resolver's three real answers rendered
as three distinct things. Choosing opens a picker over `ListAccounts` (4/9).

## What's Staying the Same

- The vault, its format and `SessionVault` — 3/9's.
- `AccountsService` — 4/9's; this node calls `ListAccounts` and adds no RPC to it.
- **Every git and GitHub call site.** Nothing in this node changes how a token is obtained for an
  operation. That is `#keyring` 9/9, deliberately last and blocked on
  [#492](https://github.com/uppin/tddy-coder/pull/492).
- `main_branch_ref`, `remote_name`, `host_repo_paths` and every other row field.

## Impact Analysis

| Package | Impact |
|---|---|
| `tddy-projects` | `ProjectData.accounts`; `SetProjectAccounts` handler; forwarding as `SetProjectDefaultBranch` does |
| `tddy-service` | `ProjectEntry.accounts`; the new RPC and its two messages |
| `tddy-accounts` | The resolver and its four answers |
| `tddy-web` | The per-provider account control on the Projects screen |
| `tddy-credentials` | **Unchanged** |

**A row written by this node is readable by an older daemon** — `serde(default)` on a new field, the
same shape the three existing optional fields use. This node's break is elsewhere and was made in
3/9: the credential store itself.

## Implementation Plan

1. `ProjectData.accounts` with its serde defaults, and the storage round-trip tests.
2. `ProjectEntry.accounts` and `SetProjectAccounts` in `project.proto`.
3. The handler: replace-whole-list, refuse two accounts for one provider, forward as
   `SetProjectDefaultBranch` does.
4. The resolver in `tddy-accounts`, with the four answers.
5. `ListProjects` carrying the assignment.
6. The Projects screen control and its picker over `ListAccounts`.
7. Cypress component tests for the three rendered states.

## Acceptance Criteria

- [ ] A project row round-trips `accounts` through `projects.yaml`; an absent field reads as empty
- [ ] `SetProjectAccounts` replaces the whole list and is forwarded to peers owning the `project_id`
- [ ] Two accounts of the same provider on one project are **refused**, with that as the reason
- [ ] The resolver returns `Assigned`, `NotAssigned` and `UnknownOnThisHost` as three distinct answers
- [ ] **An unassigned project resolves to nothing** — no fallback to the caller, to a sole account,
      or to an unauthenticated request
- [ ] `ListProjects` carries each project's assignment
- [ ] The Projects screen shows the three states distinctly and can change the assignment
- [ ] `tddy-projects` gains **no** dependency on `tddy-credentials`
- [ ] No git or GitHub call site changes in this node

## References

- [Project concept](../project-concept.md)
- [Projects screen — multi-host](../../web/projects-screen-multi-host.md)
- [Git integration base ref](../../coder/git-integration-base-ref.md) — the `main_branch_ref` precedent
- [#492](https://github.com/uppin/tddy-coder/pull/492) — what 9/9 waits on
