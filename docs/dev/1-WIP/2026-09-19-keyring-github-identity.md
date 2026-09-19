# Changeset: Git and GitHub operations resolve the project's account

**Date**: 2026-09-19
**Status**: 🚧 In Progress · ⛔ **Blocked on [#492](https://github.com/uppin/tddy-coder/pull/492)**
**Type**: Architecture Change
**Stack**: `#keyring` 9/9 · branch `feature/keyring/github-identity` · base `feature/keyring/link-github` (#515)

## Affected Packages

- **tddy-github** (**after #492**): the REST client takes a resolved token instead of reading the environment
- **tddy-accounts** (new in `#keyring` 4/9): the resolver is called here for the first time in anger
- **tddy-daemon-livekit**: [livekit-service.md](../../../packages/tddy-daemon-livekit/docs/livekit-service.md)
  - `src/session_room.rs` — the session's git environment
- **tddy-daemon-auth**: [auth-service.md](../../../packages/tddy-daemon-auth/docs/auth-service.md)
  - `FileGitHubTokenStore`'s remaining readers retired
- **tddy-tools**: no code change; its PR tools become correctly scoped by consequence

## Related Feature Documentation

- [PRD — Git and GitHub operations resolve the project's account](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-github-identity.md)
- [Cross-daemon session authentication](../../ft/daemon/session-auth.md)
- [Session rooms](../../ft/daemon/session-room.md)
- [Project concept](../../ft/daemon/project-concept.md)

## Summary

Every git and GitHub operation the daemon performs for a session uses **the account the project is
assigned** — token *and* commit identity, from one resolution.

This is the node the stack exists for. 3/9 put credentials somewhere safe, 4/9 made them visible, 5/9
let a project name one, 8/9 allowed a second to exist; none of it changes a commit's author until
here.

## Background

Today a token comes from the process environment — `github_rest_common.rs:17`, *"`GITHUB_TOKEN`
preferred, then `GH_TOKEN`"* — so every project a daemon serves acts as the same GitHub user. A
commit identity comes from the checkout's `user.email`, which on a daemon host is whatever the
machine was configured with.

One deliberate exception stays: `session_room.rs:337` signs WIP snapshots under a fixed machine
identity because *"this object is not the agent's work, it is a machine-made snapshot of it"*. That
reasoning is unaffected by a project having an account.

## Responsibility

**This node owns the resolution from project to acting GitHub identity, and the removal of every
other way one could be chosen.**

- calling 5/9's resolver at the session's edge, once, for both the token and the identity;
- a distinct failure for each `AccountResolution` outcome;
- deleting the environment-variable resolution path;
- retiring `FileGitHubTokenStore`'s remaining readers.

## Boundaries

**Owned surface:**

| Symbol | Package |
|---|---|
| the token parameter replacing environment resolution | `tddy-github` (after #492) |
| the session git environment built from one resolution | `tddy-daemon-livekit` |
| the four resolution outcomes' failure messages | `tddy-accounts` |

**Explicitly not this node's:**

- **The REST client's move** — #492's, entirely. This node neither moves a file nor claims the record.
- **The resolver** (5/9) — called, not changed.
- **Linking** (8/9) — a second account already exists by the time this node runs.
- **WIP snapshot identity** — kept as it is, with the code's own reason.
- **A credential helper** — considered, deferred, and recorded below.

**The line this node must not cross**: **no environment fallback.** Not "prefer the account, fall
back to `GITHUB_TOKEN`", not "use the environment when nothing is assigned". A fallback here means a
push that should have failed instead succeeds **as the wrong person**, to a repository that person
can write to.

## Dependencies

**Parent in the line**: `#keyring` 8/9 `link-github` — [#515](https://github.com/uppin/tddy-coder/pull/515).
**A real edge**, and the only in-stack one that also matters semantically: resolving *a* project's
account is only meaningful when more than one account can exist.

**The load-bearing edge is 5/9** [#512](https://github.com/uppin/tddy-coder/pull/512) — the
assignments and `AccountResolution`. 3/9 and 4/9 are transitive through it.

**⛔ Out-of-stack blocker: [#492](https://github.com/uppin/tddy-coder/pull/492)** — see Prerequisites.

**Dependents**: none. This is the top of the stack.

**New external dependencies: none.**

## Draft PR contract

Published in this PR's **second commit**:

**Surface**

- `tddy-github`'s REST entry points taking a token parameter (**signatures only**, and written against
  the post-#492 module paths);
- the session git-environment constructor's signature in `tddy-daemon-livekit`.

**Failing tests**

- a commit made in a session carries the assigned account's name and email;
- a GitHub API call uses the assigned account's token;
- two projects on one daemon, assigned different accounts, act as different GitHub users;
- token and identity always come from **one** resolution — a test that fails if they are resolved
  independently;
- `NotAssigned` fails GitHub operations with that reason, and **`GITHUB_TOKEN` in the daemon's
  environment does not rescue it**;
- `UnknownOnThisHost` fails with *that* reason, distinct from `NotAssigned`;
- `Ambiguous` fails rather than picking;
- WIP snapshot commits are still authored by `tddy-daemon`.

⚠ **Not mergeable in that state** — implementation follows in this same PR, **after #492 merges**.

## Green wave

**Wave 5 of 5** — alone.

```
waves   1:{n1}   2:{n2, n3}   3:{n4}   4:{n5, n6, n7, n8}   5:{n9}
line    n1 · n2, n3 · n4 · n5, n6, n7, n8 · n9
```

Nothing depends on it, and it depends on all of wave 4 plus an out-of-stack PR. It is last in the
line and last in time, which is what the developer asked for.

## Prerequisites

### ⛔ BLOCKING — the GitHub REST client is mid-move — [`squatting-github-rest-client`](../../../packages/tddy-workflow-recipes/docs/code-issues/squatting-github-rest-client.md)

**Status in the record: `Open — claimed by #492, in flight`.** It is another stack's to fix
(`#carve` 6/10), and this node neither claims it nor touches the files.

The record's own guidance is the reason this node waits:

> Coordinate if you are **adding a REST call** — it belongs in `tddy-github` after #492, and adding
> it here means #492 moves it too.

This node does something stronger than add a call: it changes how **all 2,124 lines** of that client
get their token. Doing it first means editing a crate those files are leaving, and then having #492
rewrite the result.

**The developer decided this before the stack was cut**, which is why it is recorded rather than
escalated:

> plan the changes to git/github as late as possible with a dependency of this PR being merged
> https://github.com/uppin/tddy-coder/pull/492

⚠ **Nodes 1–8 are unaffected.** None of them touches `tddy-workflow-recipes`, so the block costs this
node's schedule and nothing else. This PR stays a draft until #492 merges.

### ⚠ DURING — `runtime::build` complexity — [`complexity-runtime-build`](../../../packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md)

Wiring the resolver into the session path passes through it. Recorded, not claimed.

### ℹ ANSWERED — what replaces `FileGitHubTokenStore`

Its doc comment states the constraint that produced it:

> The daemon runs from a systemd unit with no `GITHUB_TOKEN` in its environment, so the only
> credential it can use for an operator's PR reads is the one that operator granted at login.

Correct, and narrower than what the stack now needs: the file is `login -> access_token`, one token
per login name, with no notion of a project. 3/9 replaced the storage; this node removes the last
readers. Answered, so the entry is recorded and **not** deleted.

### Unanalyzed packages

`tddy-daemon-livekit` and `tddy-tools`' code-issue records were not scanned for this node beyond the
two above. **"Not measured" is not "clean"**; this node claims nothing about them.

## Scope

- [x] **PRD**: [PRD-2026-09-19-keyring-github-identity.md](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-github-identity.md)
- [x] **Changeset**: this document
- [ ] ⛔ **Blocked**: #492 must merge before implementation begins — the draft-PR contract is written
      against the post-#492 module paths and is the only work that proceeds before it
- [ ] **Draft PR contract**: surface + failing tests (wave 2, commit 2)
- [ ] **Resolution**: one call, token and identity from one answer
- [ ] **Outcomes**: a distinct failure per `AccountResolution` variant
- [ ] **Deletion**: the environment resolution path; `FileGitHubTokenStore`'s readers
- [ ] **Testing**: unit + acceptance, scoped
- [ ] **Package Documentation**: `packages/tddy-accounts/docs/github-identity-resolution.md`
- [ ] **Code Quality**: scoped clippy; CI green

## Technical Changes

### State A (Current)

- The REST client resolves `GITHUB_TOKEN` then `GH_TOKEN` from the **process** environment — one
  GitHub identity per daemon, for every project it serves.
- `FileGitHubTokenStore` keeps `login -> access_token`, unaware of projects.
- The agent's commits carry whatever identity the checkout is configured with.

### State B (Target)

- A session's git environment carries the **assigned account's** token, name and email, from one
  resolution.
- No environment fallback exists. An unassigned project cannot perform a GitHub operation.
- `FileGitHubTokenStore` has no readers.
- WIP snapshots are still authored by `tddy-daemon`.

### Delta (What's Changing)

#### tddy-github (after #492)
- **API**: REST entry points take a token. `github_token_from_env` and its callers are **deleted** —
  not deprecated, because a deprecated environment read is still an environment read.

#### tddy-daemon-livekit
- **Implementation**: the session's environment is built from one `AccountResolution`. The token and
  the four `GIT_*` variables come from the same record, in one place, so a mismatch is not
  constructible.
- **Unchanged**: `WIP_COMMIT_IDENTITY_NAME` / `_EMAIL` and the `commit-tree` identity block.

#### tddy-accounts
- **Implementation**: one failure message per `AccountResolution` variant, each naming what the person
  would do about it.

#### tddy-daemon-auth
- **Implementation**: `FileGitHubTokenStore`'s remaining readers removed.

### Decision recorded with its alternative

**The environment variable is the seam.** `git` and `gh` are ordinary subprocesses; setting
`GITHUB_TOKEN` and the four `GIT_*` variables makes every invocation correct without either tool
learning anything about this stack.

⚠ **Worth challenging**: the variable is visible to every process in the session, including the agent,
which can print it. A credential helper handing a token to `git` per invocation never exposes it —
but it is considerably larger, and it does not cover `gh` or the agent's own API calls, which *need*
the token. Recorded so the reviewer can weigh it; a helper can be added later without changing the
resolution this node builds.

## Implementation Milestones

⛔ **M1 begins only after #492 merges.**

- [ ] **M1** — REST entry points take a token; environment resolution deleted
- [ ] **M2** — one resolution at the session edge; token + identity from it
- [ ] **M3** — a distinct failure per outcome
- [ ] **M4** — retire `FileGitHubTokenStore`'s readers
- [ ] **M5** — acceptance: two projects, two accounts, one daemon
- [ ] **M6** — `packages/tddy-accounts/docs/github-identity-resolution.md`

## Testing Plan

### Testing Strategy

**Two tests carry this node, and both are negative.**

The first: with a project that has **no** account assigned and `GITHUB_TOKEN` set in the daemon's
environment, a GitHub operation **fails**. Every positive test in this plan passes with a fallback in
place; this is the only one that does not, and what it prevents is a push succeeding as the wrong
person.

The second: the token and the commit identity come from **one** resolution. Resolving twice works
perfectly until an assignment changes between the two calls, and then produces a commit authored by
one account and pushed by another — which reads, to everyone downstream, as the first account's work.

### Unit tests

- Each `AccountResolution` variant maps to its own failure, with its own message.
- The session environment is constructed from a single record — asserted by construction, not by
  comparing two lookups.
- No code path reads `GITHUB_TOKEN` or `GH_TOKEN` from the process environment.

### Acceptance tests

- A commit in a session carries the assigned account's name and email.
- A GitHub API call uses the assigned account's token.
- Two projects, two accounts, one daemon: different GitHub users.
- `NotAssigned` + `GITHUB_TOKEN` exported → **fails**.
- `UnknownOnThisHost` fails with its own reason.
- WIP snapshot commits are still authored by `tddy-daemon`.

### Verification scope

`./test -p tddy-accounts -p tddy-github -p tddy-daemon-livekit -p tddy-daemon-auth` and scoped
clippy. LiveKit-backed tests reuse the testkit container. Whole-workspace green comes from CI via
`scripts/ci-status.sh`.

## Acceptance Criteria

- [ ] A commit carries the assigned account's name and email
- [ ] A GitHub API call uses the assigned account's token
- [ ] Two projects assigned different accounts act as different GitHub users
- [ ] Token and identity always come from **one** resolution
- [ ] `NotAssigned` fails; **no environment fallback**
- [ ] `UnknownOnThisHost` fails with its own reason
- [ ] `Ambiguous` fails rather than picking
- [ ] `GITHUB_TOKEN` in the daemon's environment is never used for a session
- [ ] WIP snapshot commits are still authored by `tddy-daemon`
- [ ] `FileGitHubTokenStore` has no readers left

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Publish the draft-PR contract — wave 2
- [ ] ⛔ Wait for [#492](https://github.com/uppin/tddy-coder/pull/492)
- [ ] M1–M6
- [ ] `packages/tddy-accounts/docs/github-identity-resolution.md`
- [ ] `/wrap-context-docs` — this node claims **no** `docs/dev/todo/` entry and **no** code-issue
      record. `squatting-github-rest-client.md` belongs to **#492** and must not be deleted here
