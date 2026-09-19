# Changeset: Project → account assignments

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: `#keyring` 5/9 · branch `feature/keyring/assignments` · base `feature/keyring/accounts` (#511)

## Affected Packages

- **tddy-projects**: [project-service.md](../../../packages/tddy-projects/docs/project-service.md)
  - `src/project_storage.rs:10` — `ProjectData.accounts`
  - `src/handler.rs`, `src/service.rs` — `SetProjectAccounts`, forwarded as
    `SetProjectDefaultBranch` is
- **tddy-service**: `proto/project.proto` — `ProjectEntry.accounts`, the new RPC and its messages
- **tddy-accounts**: `docs/accounts-service.md` — the resolver and its four answers
- **tddy-web**: [projects-screen.md](../../../packages/tddy-web/docs/projects-screen.md)
  - `src/components/projects/` — the per-provider account control

## Related Feature Documentation

- [PRD — Project → account assignments](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-assignments.md)
- [Project concept](../../ft/daemon/project-concept.md)
- [Projects screen — multi-host](../../ft/web/projects-screen-multi-host.md)
- [Git integration base ref](../../ft/coder/git-integration-base-ref.md)

## Summary

A project row gains a list of assigned account identifiers, an RPC to set them, a control on the
Projects screen, and a resolver that answers "which account does this project use for this provider"
— returning **nothing**, explicitly, when none is assigned.

## Background

`ProjectData` (`project_storage.rs:10`) already carries three optional fields added the same way this
one is: `#[serde(default, skip_serializing_if = …)]`, a documented behaviour when absent, and — for
`main_branch_ref` — a `SetProjectDefaultBranch` RPC that `project.proto:22-24` describes as
*"logical-project scope: forwarded to peer hosts owning the same project_id"*. This node follows that
precedent exactly rather than inventing a shape.

What is genuinely new is a consequence of 3/9: an `AccountId` is **daemon-minted** and lives in that
daemon's vault, so an assignment forwarded to a peer names an account that peer does not have — and
will not until 6/9 propagates the vault. That state needs a name of its own; calling it "not
assigned" would show a person their own choice as absent, on another host, with no explanation.

## Responsibility

**This node owns which account a project uses, and what happens when none does.**

- `ProjectData.accounts` and its storage round-trip;
- `SetProjectAccounts` and its forwarding, including the refusal of two accounts for one provider;
- the resolver's four answers, and in particular that `NotAssigned` resolves to **nothing**;
- the Projects-screen control that makes and shows the assignment.

## Boundaries

**Owned surface:**

| Symbol | Package |
|---|---|
| `ProjectData.accounts` | `tddy-projects` |
| `SetProjectAccounts` + its messages; `ProjectEntry.accounts` | `tddy-service` (proto) |
| `AccountResolution` and the resolver | `tddy-accounts` |
| the Projects-screen account control | `tddy-web` |

**Explicitly not this node's:**

- **Every git and GitHub call site.** Nothing here changes how a token is obtained for an operation —
  that is `#keyring` 9/9, deliberately last and blocked on
  [#492](https://github.com/uppin/tddy-coder/pull/492). This node ships the assignment and the
  answer; 9/9 is what acts on it.
- **The vault** (3/9) and **`AccountsService`** (4/9) — called, not extended.
- **Making an assignment meaningful on a peer host** — 6/9. Until then `UnknownOnThisHost` is the
  honest answer, and this node's job is to have that answer exist.
- **Linking accounts** — 7/9 and 8/9.

**Two lines this node must not cross:**

1. **No fallback for an unassigned project.** Not the caller's own login, not "the only account in
   the vault", not an unauthenticated request. The developer's instruction is explicit, and a
   fallback here would push commits under the wrong identity — the exact failure the stack exists to
   prevent.
2. **`tddy-projects` must not depend on `tddy-credentials`.** The resolver lives in `tddy-accounts`,
   which already depends on the store. A crate about repositories, branches and checkout paths would
   otherwise put the credential store on every project consumer's dependency path.

## Dependencies

**Parent in the line**: `#keyring` 4/9 `accounts` — [#511](https://github.com/uppin/tddy-coder/pull/511).
A **real edge**: the picker is `ListAccounts`, the resolver lives in `tddy-accounts`, and
`UnknownOnThisHost` is answerable only against a vault.

**Transitively**: 3/9 [#510](https://github.com/uppin/tddy-coder/pull/510), 2/9
[#509](https://github.com/uppin/tddy-coder/pull/509), 1/9
[#508](https://github.com/uppin/tddy-coder/pull/508).

**Dependents**: 9/9 `github-identity` — one, and it is the node that consumes the resolver for real.

**New external dependencies: none.** `serde`, `serde_yaml` and the proto toolchain are in place.

## Draft PR contract

Published in this PR's **second commit**:

**Surface**

- `ProjectData.accounts` — real, with its serde attributes; it is a field, and stubbing it would
  mean stubbing `serde`.
- `project.proto`: `ProjectEntry.accounts = 8`, `SetProjectAccounts` and its two messages; generated
  code committed.
- `tddy-accounts`: `AccountResolution` and `resolve_account(...)`, signature only, body `todo!()`.

**Failing tests**

- a row round-trips `accounts` through `projects.yaml`; an absent field reads as empty;
- `SetProjectAccounts` replaces the whole list;
- two accounts of one provider are **refused**, with that as the reason;
- the RPC is forwarded to peers owning the same `project_id`, as `SetProjectDefaultBranch` is;
- the resolver returns `Assigned`, `NotAssigned` and `UnknownOnThisHost` as three distinct answers;
- **an unassigned project resolves to `NotAssigned` and to nothing else** — the no-fallback test;
- `ListProjects` carries each project's assignment;
- Cypress component: the three states render distinctly and the picker changes the assignment.

⚠ **Not mergeable in that state** — implementation follows in this same PR.

## Green wave

**Wave 4 of 5**, with 6/9, 7/9 and 8/9 — all four unblocked by 4/9 landing, and none depending on
another.

```
waves   1:{n1}   2:{n2, n3}   3:{n4}   4:{n5, n6, n7, n8}   5:{n9}
line    n1 · n2, n3 · n4 · n5, n6, n7, n8 · n9
```

**Intra-wave sort — blockers first.** This node leads wave 4 with **one** transitive dependent (9/9);
6/9, 7/9 and 8/9 have **none**. The three behind it are ordered by how foundational they are, not by
what waits on them, because nothing does.

## Prerequisites

### ⚠ DURING — PR-stack status polling and stack hygiene — [`2026-07-26-pr-stack-status-polling-and-stack-hygiene.md`](../todo/2026-07-26-pr-stack-status-polling-and-stack-hygiene.md)

Project-resolved credentials are what would eventually change how PR status obtains a token. This
node deliberately does not touch that call site — 9/9 does — so the entry is recorded and **not
fixed**. Noted here because the connection is easy to make and acting on it would pull 9/9's blocked
work forward.

### Unanalyzed packages

`tddy-projects` has one code-issue record; it is not in this node's path (the storage functions this
node extends are small and not among the measured ones). `tddy-accounts` is new (4/9).
`tddy-service` has no `docs/` directory, so nothing there has been measured — **"not measured" is not
"clean"**.

## Scope

- [x] **PRD**: [PRD-2026-09-19-keyring-assignments.md](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-assignments.md)
- [x] **Changeset**: this document
- [ ] **Draft PR contract**: field + proto + resolver signature + failing tests (wave 2, commit 2)
- [ ] **Storage**: `ProjectData.accounts` and its round-trip
- [ ] **Proto + handler**: `SetProjectAccounts`, forwarding, the same-provider refusal
- [ ] **Resolver**: four answers in `tddy-accounts`
- [ ] **Web**: the per-provider account control and its picker
- [ ] **Testing**: storage, handler, resolver, Cypress component
- [ ] **Package Documentation**: `tddy-projects`, `tddy-accounts`, `tddy-web`
- [ ] **Code Quality**: scoped clippy; CI green

## Technical Changes

### State A (Current)

- `ProjectData` has no notion of an account. Credentials are resolved by the **caller's** GitHub
  login, wherever they are resolved at all.
- `ProjectService` has five RPCs; `SetProjectDefaultBranch` is the shape a sixth would follow.
- The Projects screen shows name, git URL, host and default branch.

### State B (Target)

- A project row carries assigned account identifiers, forwarded to every host owning the project.
- A resolver answers `Assigned` / `NotAssigned` / `UnknownOnThisHost` / `Ambiguous`, and
  `NotAssigned` resolves to nothing.
- The Projects screen makes and shows the assignment, with the three real answers rendered as three
  distinct things.

### Delta (What's Changing)

#### tddy-projects
- **Storage**: `accounts: Vec<String>`, `#[serde(default, skip_serializing_if = "Vec::is_empty")]`.
  An older daemon reads a newer row, exactly as with the three existing optional fields.
- **Handler**: `SetProjectAccounts` — replace-whole-list, so two concurrent edits cannot interleave
  into a set neither person chose; refuses two accounts of one provider; forwarded to peers owning
  the same `project_id`.
- **Not changed**: `main_branch_ref`, `remote_name`, `host_repo_paths`, and the provisioning path.

#### tddy-service
- **API**: `ProjectEntry.accounts = 8`; `SetProjectAccounts` with
  `{ session_token, project_id, accounts, daemon_instance_id }`.

#### tddy-accounts
- **API**: `AccountResolution { Assigned, NotAssigned, UnknownOnThisHost, Ambiguous }` and the
  resolver over a project's assignment plus this host's vault.

#### tddy-web
- **UI**: a per-provider account control on each project row; the picker reads `ListAccounts`.

## Implementation Milestones

- [ ] **M1** — `ProjectData.accounts` + storage round-trip
- [ ] **M2** — proto: `ProjectEntry.accounts`, `SetProjectAccounts`, generated code
- [ ] **M3** — handler: replace-whole-list, same-provider refusal, forwarding
- [ ] **M4** — the resolver and its four answers
- [ ] **M5** — `ListProjects` carries the assignment
- [ ] **M6** — Projects-screen control and picker
- [ ] **M7** — documentation for the three packages

## Testing Plan

### Testing Strategy

**Three levels, each where its risk lives.** Storage is a serde round-trip over a temp directory —
unit. Forwarding and the same-provider refusal are handler behaviour — integration against the
service. That the three resolver answers look different to a person is a DOM property — Cypress
component, with `mountWithRpc` + `anInMemoryRpcBackend` per the house rule.

**The no-fallback test is the one that matters most**, and it is written as an assertion about what
the resolver does *not* do: given a vault holding exactly one GitHub account and a project with no
assignment, the answer is `NotAssigned`. A resolver that helpfully returned the sole account would
pass every other test in this plan.

### Unit tests (`tddy-projects`)

- A row round-trips `accounts`; a row written without the field reads as empty.
- `write_projects` preserves `accounts` for rows it is not editing.

### Integration tests (`tddy-projects` service)

- `SetProjectAccounts` replaces the whole list rather than merging.
- Two accounts of the same provider are refused, and the reason names the provider.
- The call is forwarded to peer hosts owning the same `project_id`.
- `ListProjects` carries the assignment.

### Unit tests (`tddy-accounts`)

- `Assigned` when exactly one account for the provider is present in this host's vault.
- `NotAssigned` when none is assigned — **including when the vault holds exactly one candidate**.
- `UnknownOnThisHost` when assigned but absent from this host's vault.
- `Ambiguous` is reachable only by constructing the state the RPC refuses.

### Web tests

- Cypress component: assigned, not assigned and unavailable-here render distinctly.
- Cypress component: the picker issues `SetProjectAccounts` with the full list.

### Verification scope

`./test -p tddy-projects -p tddy-accounts -p tddy-service` and scoped clippy. For `tddy-web`, **the
single spec and story under change** — a full Cypress e2e run is ~50 minutes over 207 specs and
belongs to CI. Whole-workspace green comes from CI via `scripts/ci-status.sh`.

## Acceptance Criteria

- [ ] `accounts` round-trips through `projects.yaml`; an absent field reads as empty
- [ ] `SetProjectAccounts` replaces the whole list and is forwarded to peers owning the `project_id`
- [ ] Two accounts of one provider are refused, with the provider named
- [ ] `Assigned`, `NotAssigned` and `UnknownOnThisHost` are three distinct answers
- [ ] **An unassigned project resolves to nothing, with a sole vault account present**
- [ ] `ListProjects` carries the assignment
- [ ] The Projects screen shows the three states distinctly and can change the assignment
- [ ] `tddy-projects` gains no dependency on `tddy-credentials`
- [ ] No git or GitHub call site changes in this node

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Publish the draft-PR contract — wave 2
- [ ] M1–M7
- [ ] Package documentation for `tddy-projects`, `tddy-accounts`, `tddy-web`
- [ ] `/wrap-context-docs` — this node claims **no** backlog entry and **no** code-issue record
