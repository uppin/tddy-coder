# Changeset: Git and GitHub operations resolve the project's account

**Date**: 2026-09-19
**Status**: 🚧 In Progress · ✅ **#492 is this stack's base** — see Prerequisites
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

**#492 is an ancestor. ⚠ It is not yet a *green* one.** On 2026-09-19 the whole `#keyring` stack
was re-based onto [#492](https://github.com/uppin/tddy-coder/pull/492) `feature/carve/git-plumbing`,
which removes the merge-order collision this node was planned around. What the plan then asserted —
that "the moved `tddy-github` is present in this tree from node 1 onward" — **is false today**, and
was false when it was written: #492 is itself in its wave-2 red state. Measured on this branch on
2026-09-20:

- `packages/tddy-git` does not exist;
- `packages/tddy-github/src/` is `auth_service.rs`, `lib.rs`, `provider.rs`, `real.rs`,
  `session_token.rs`, `session_token_v2.rs`, `stub.rs`, `token_store.rs` — no `github_pr`, no
  `github_rest_common`, no PR surface of any kind;
- `packages/tddy-github/tests/git_plumbing_shape.rs` is #492's **failing** acceptance suite, and its
  four assertions are among this branch's inherited red;
- all 2,124 lines of the REST client, and all ten callers of `github_token_from_env`, are still in
  `tddy-workflow-recipes`.

**Measured again on 2026-09-20, after the stack-wide rebase: the move has landed.** `#carve` pushed
`#carve 6/11` — *"tddy-git is born, and GitHub REST moves to the crate named after the service"* —
onto `feature/carve/git-plumbing`, and the cascade brought every layer of this stack onto it. At this
branch's tip `packages/tddy-git/src/` holds `lib.rs` and `ssh_exec.rs`, and the same 2,124 lines now
sit in `packages/tddy-github/src/` as `github_pr.rs` (494), `github_rest_common.rs` (338) and
`pr_api.rs` (1,292). `github_token_from_env` is **defined** at `github_rest_common.rs:19`, with 14
call sites across five files.

So the post-#492 module paths this node's contract was written against are **real on this branch
now**. What that does not change: #492 is still red, and still moving — it was `5/9` when this node
was planned and is `6/11` today. The REST half of the contract was published as deferred before the
base moved, and the deferral stands with a better reason than the one it was written with: green does
that work against paths that exist, rather than wave 2 doing it against paths that were about to be
rewritten.

The consequence is scheduling, not design. Nothing about *where* the token parameter belongs
changed.

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

⚠ **Not mergeable in that state** — implementation follows in this same PR.

### ⚠ Plan correction — the REST half could not be published

The contract's first surface bullet promised `tddy-github`'s REST entry points taking a token
parameter, *"written against the post-#492 module paths"*. **When the surface was published those
paths did not exist**: #492 was an ancestor in its own wave-2 red state, the client was still 2,124
lines in `tddy-workflow-recipes`, and `packages/tddy-git` had not been created. The
`## Dependencies` claim that the move *"has already happened in this tree"* was corrected above.

**Hours later the base moved and the move landed** — see the 2026-09-20 measurement under
`## Dependencies`. The paths are real on this branch now. That vindicates the deferral rather than
reversing it: had the parameter been published against the pre-move files, every one of them would
have been rewritten underneath it by `#carve 6/11`.

**What was done instead.** Publishing the parameter against the pre-move paths was rejected: it means
editing the exact files #492 is moving, which is what `## Boundaries` puts out of scope (*"The REST
client's move — #492's, entirely"*) and what basing the stack on #492 existed to avoid. So the REST
half is **deferred to this node's green phase**, which now runs against real paths. Two
consequences, both recorded rather than hidden:

- acceptance criterion *"a GitHub API call uses the assigned account's token"* is covered in wave 2
  only as far as `ActingIdentity::token` — the assertion at the REST boundary arrives with M1;
- no test in this commit constrains where the parameter lands. M1 is unchanged in scope; it simply
  has no red test in front of it yet.

**Nothing about the design changed.** The token still comes from one resolution, and the
no-environment-fallback rule is pinned by
`a_project_that_assigns_no_account_is_refused_although_the_environment_holds_a_token`, which sets
`GITHUB_TOKEN` in the process and asserts the refusal anyway.

### As published — measured red (wave 2, commit 2)

Scoped to this node's own verification scope — `./test -p tddy-accounts -p tddy-github
-p tddy-daemon-livekit -p tddy-daemon-auth --no-fail-fast`, run on 2026-09-20 before and after the
surface commit. **Scoped, not whole-workspace**; CI is the authority on everything else.

| | passed | failed |
|---|---|---|
| Baseline, before this commit | 279 | 76 |
| After publishing the surface | 280 | 95 |

**+19 failing, +1 passing, 0 previously-failing tests fixed or hidden.** The 76 inherited failures
are nodes 2–8's unimplemented green phases plus #492's own four `git_plumbing_shape.rs` assertions;
every one of them is still failing, by name, for the same reason.

Where the 19 land, and what each fails on:

| Suite | New red | Fails at |
|---|---|---|
| `tddy-accounts/tests/project_resolved_identity_acceptance.rs` | 5 | `identity.rs` — `acting_identity` |
| `tddy-accounts/tests/acting_identity_unit.rs` | 8 | `identity.rs` — `acting_identity` (5) and `IdentityError: Display` (3) |
| `tddy-daemon-livekit/tests/session_git_identity_acceptance.rs` | 3 | `session_git.rs` — `session_git_environment` |
| `tddy-daemon-auth/tests/login_time_token_store_is_retired.rs` | 3 | their own assertions — the readers are still there |

Sixteen of the nineteen panic on one of this node's three `todo!()`s and on nothing else; the other
three are the structural assertions about `FileGitHubTokenStore`'s remaining readers, which fail
because those readers exist.

**One test is green from the moment it was written**, and is recorded rather than manufactured into
red: `a_work_in_progress_snapshot_is_still_signed_by_the_daemon_itself`. It guards what this node
promised **not** to change, so red would mean the promise was already broken. Proven non-vacuous by
mutation — renaming `WIP_COMMIT_IDENTITY_NAME` to a person's login fails it, and restoring the
constant passes it again.

### Design decisions taken while publishing the surface

1. **One entry point, `acting_identity`, returning one `ActingIdentity` carrying both halves.** The
   alternative — a `token_for` beside an `identity_for` — is what the contract bullet *"token and
   identity always come from one resolution"* rules out, and a shape no test can fully police. There
   is nothing to police here: the crate publishes no way to obtain one without the other.
2. **The git identity is derived from the record's provider metadata, never from its label.**
   `META_SUBJECT_ID` and `META_SUBJECT` (`#keyring` 8/9) give `101+ada@users.noreply.github.com` and
   `ada`; `CredentialRecord::label` is the one mutable field and is deliberately not identity-bearing
   — `renaming_an_account_does_not_change_who_its_commits_are_authored_by` pins that.
3. **A fifth refusal, `IdentityError::Unusable`, beyond the four `AccountResolution` outcomes.** A
   record can resolve and still carry no provider identifiers. Refusing it — naming the absent
   metadata key — is the alternative to inventing an address, and a commit authored under an invented
   address is attributed to nobody and discovered only by reading history.
4. **`session_git_environment` returns pairs rather than applying them to a `Command`.** The caller
   that spawns the agent is the only thing that knows which process they belong on, and a list of
   pairs is something a test can read without running `git`.
5. **`tddy-daemon-livekit` gained a path dependency on `tddy-accounts`.** No external dependency, and
   no cycle: `tddy-accounts` names `tddy-credentials`, `tddy-rpc` and `tddy-service`, none of which
   reach back. The direction is the point — the crate deciding which account a project acts as knows
   nothing about sessions or LiveKit.

⚠ **`GITHUB_TOKEN` inside a session is a separate question, and this commit does not settle it.**
`github_token_from_env` is defined in `tddy-github/src/github_rest_common.rs` and called from 14
sites across five files, and some of them run inside the *agent's* process, where the daemon may
legitimately be the thing that set the variable.
Deleting the daemon's fallback and deleting the agent's delivery mechanism are different changes; no
test published here forces the second, and M1 must not quietly become it.

## Green wave

**Wave 5 of 5** — alone.

```
waves   1:{n1}   2:{n2, n3}   3:{n4}   4:{n5, n6, n7, n8}   5:{n9}
line    n1 · n2, n3 · n4 · n5, n6, n7, n8 · n9
```

Nothing depends on it, and it depends on all of wave 4 plus an out-of-stack PR. It is last in the
line and last in time, which is what the developer asked for.

## Prerequisites

### ⚠ UNBLOCKED BY THE BASE, NOT YET RESOLVED BY IT — the GitHub REST client is mid-move — [`squatting-github-rest-client`](../../../packages/tddy-workflow-recipes/docs/code-issues/squatting-github-rest-client.md)

**Status in the record: `Open — claimed by #492, in flight`.** It is another stack's to fix
(`#carve` 6/10), and this node neither claims it nor touches the files — **it must not delete the
record**, which is #492's to delete when it wraps.

This was a ⛔ BLOCKING entry when the stack was cut. Re-basing the `#keyring` root from `master`
onto `feature/carve/git-plumbing` (#492) on 2026-09-19 removed the *collision* — this node will never
edit a crate the files are leaving. ⚠ **It did not move the files.** #492 is an ancestor in its own
red state, so on 2026-09-20 this tree still has the client in `tddy-workflow-recipes` and
`tddy-github` has no PR surface. The entry therefore stays **⚠ DURING**: it constrains this node's
green phase, which must thread the token parameter through wherever #492 has put the file by then.

The record's own guidance is what made it blocking:

> Coordinate if you are **adding a REST call** — it belongs in `tddy-github` after #492, and adding
> it here means #492 moves it too.

This node does something stronger than add a call: it changes how **all 2,124 lines** of that client
get their token. Doing it *before* the move would mean editing a crate those files are leaving, and
then having #492 rewrite the result. Basing the stack on #492 removes that collision entirely rather
than scheduling around it.

**The developer decided this before the stack was cut**, which is why it is recorded rather than
escalated:

> plan the changes to git/github as late as possible with a dependency of this PR being merged
> https://github.com/uppin/tddy-coder/pull/492

⚠ **The cost moved from this node to the whole stack.** Under the original plan nodes 1–8 were
independent of #492 and only this node waited. Now every `#keyring` node lands behind `#carve` 3/10
[#498](https://github.com/uppin/tddy-coder/pull/498), 4/10
[#491](https://github.com/uppin/tddy-coder/pull/491) and 5/10
[#492](https://github.com/uppin/tddy-coder/pull/492). That trade was taken deliberately — it is the
cheapest possible moment for the rebase, since every `#keyring` node is still docs-only — and it is
recorded in node 1's changeset.

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
- [x] ⚠ **Unblocked by the base, not yet supplied by it**: #492 is an ancestor, so the merge-order
      collision is gone — but it is still red, so the post-move module paths are **not** in this tree
      and the REST half of the contract is deferred to this node's green phase
- [x] **Draft PR contract**: surface + failing tests (wave 2, commit 2) — ⚠ **partly**: the REST
      half is deferred, see **As published** under `## Draft PR contract`
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

✅ **M1 is not gated on #492 merging** — #492 is this stack's base, so its move is already in the tree.

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
- [x] Publish the draft-PR contract — wave 2 (⚠ REST half deferred to green)
- [x] ⚠ #492 is the stack's base — no separate wait, but it is still red: the moved module paths
      arrive only when it greens
- [ ] M1–M6
- [ ] `packages/tddy-accounts/docs/github-identity-resolution.md`
- [ ] `/wrap-context-docs` — this node claims **no** `docs/dev/todo/` entry and **no** code-issue
      record. `squatting-github-rest-client.md` belongs to **#492** and must not be deleted here
