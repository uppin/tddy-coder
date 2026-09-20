# PRD: Git and GitHub operations resolve the project's account

**Date**: 2026-09-19
**Status**: 🚧 In Progress · ✅ **#492 is this stack's base**
**Stack**: `#keyring` 9/9 · branch `feature/keyring/github-identity` · base `feature/keyring/link-github` (#515)

## Affected Features

- [Cross-daemon session authentication](../session-auth.md)
- [Session rooms](../session-room.md)
- [Project concept](../project-concept.md)
- **Per-project GitHub identity** — `docs/ft/daemon/github-identity.md`, written by this change

## Summary

Makes every git and GitHub operation the daemon performs on a session's behalf use **the account the
project is assigned**, rather than whatever token the process environment happens to hold and
whatever `user.email` the checkout happens to carry.

This is the node the whole stack was built for. `#keyring` 3/9 put credentials somewhere safe, 4/9
made them visible, 5/9 let a project name one, 8/9 allowed more than one to exist — and none of that
changes a single commit's author until this node.

## #492 is this stack's base

The developer's instruction is explicit:

> plan the changes to git/github as late as possible with a dependency of this PR being merged
> https://github.com/uppin/tddy-coder/pull/492

[#492](https://github.com/uppin/tddy-coder/pull/492) (`#carve` 6/10, draft) moves the GitHub REST
client — `github_rest_common.rs`, `github_pr.rs`, `orchestrate_pr_stack/github.rs`, **2,124 lines** —
out of `tddy-workflow-recipes` and into `tddy-github`. Its code-issue record,
`squatting-github-rest-client.md`, states the overlap directly:

> Coordinate if you are **adding a REST call** — it belongs in `tddy-github` after #492, and adding
> it here means #492 moves it too.

This node changes **how every one of those calls gets its token**. Doing it before the move lands
would mean editing 2,124 lines that are mid-move, in a crate they are leaving, and then watching the
move rewrite the result. The record is 🚧 **Claimed by #492**, which is another stack's to fix; this
node neither claims nor touches it, and **must not delete the record** — #492 deletes it when it
wraps.

**How that instruction is honoured.** On 2026-09-19 the `#keyring` root (#508) was re-based from
`master` onto `feature/carve/git-plumbing` — #492's own branch. The dependency the developer asked
for is therefore expressed as the stack's **base** rather than as a wait: the moved `tddy-github` is
present in the tree from node 1 onward, this node writes against the post-move module paths
directly, and #492 is by construction merged before any `#keyring` node can be.

**Consequence for the stack**: the cost moved from this node to all nine. Under the original plan
nodes 1–8 were independent of #492; now every node lands behind `#carve` 4/10
[#498](https://github.com/uppin/tddy-coder/pull/498), 5/10
[#491](https://github.com/uppin/tddy-coder/pull/491) and 6/10
[#492](https://github.com/uppin/tddy-coder/pull/492). The trade was taken deliberately, while every
`#keyring` node was still docs-only and the rebase was therefore free.

## Background

### Where a token comes from today

`github_rest_common.rs:17` resolves it from the process environment:

```rust
/// Resolve GitHub token from the environment (`GITHUB_TOKEN` preferred, then `GH_TOKEN`).
```

That is one token for the whole process, so every project a daemon serves acts as the same GitHub
user — and on a server daemon, as whichever user's token happened to be exported.

Meanwhile `github_token_store.rs` persists `login -> access_token`, one token per login name, for
"an operator's PR reads". Two mechanisms, neither of them aware of a project.

### Where a commit identity comes from today

Nowhere, for the agent's own commits: the checkout's `user.name` / `user.email` decide, which on a
daemon host is whatever the machine was configured with.

**One deliberate exception, which this node keeps.** `session_room.rs:337` signs WIP snapshots under
a fixed machine identity, and says why:

> Signed by the daemon under a fixed identity rather than by whatever `user.email` the checkout
> happens to carry: this object is not the agent's work, it is a machine-made snapshot of it […]

That reasoning does not weaken when a project has an account — a snapshot is still not the person's
work. `tddy-daemon` stays the author of WIP commits.

## Proposed Changes

### Resolution happens once, at the session's edge

`#keyring` 5/9's resolver answers `AccountResolution` for a `(project, provider)` pair. This node
calls it when a session's git environment is constructed, and uses the answer for **both** the token
and the identity:

| From the resolved account's record | Into |
|---|---|
| the access token | `GITHUB_TOKEN` in the session's environment, and the REST client's auth |
| the GitHub user's name | `GIT_AUTHOR_NAME`, `GIT_COMMITTER_NAME` |
| the GitHub user's email | `GIT_AUTHOR_EMAIL`, `GIT_COMMITTER_EMAIL` |

One resolution, one account, both uses. A token from account A with a commit identity from account B
is a category of bug this node must make unrepresentable, not merely avoid.

### Each resolution outcome has a distinct behaviour

| `AccountResolution` | What happens |
|---|---|
| `Assigned(id)` | that account's token and identity are used |
| `NotAssigned` | **no** `GITHUB_TOKEN` is set, and git identity is left unset — GitHub operations fail, saying the project has no account assigned |
| `UnknownOnThisHost(id)` | operations fail, saying *this host* does not hold that account's record yet — 6/9 propagation is pending, which is a different problem with a different fix |
| `Ambiguous { .. }` | operations fail; the developer's requirement is that assignment is explicit |

**There is no environment fallback.** `GITHUB_TOKEN` from the daemon's environment is not consulted,
not preferred, not used when the project has no account. The developer's instruction on the stack is
*"The change can be breaking, don't add fallbacks"*, and CLAUDE.md's standing rule is that fallbacks
make the system unsafe. Here the unsafety is concrete: a fallback means a push that should have
failed instead succeeds **as the wrong person**, to a repository that person can write to.

### The environment variable is the seam

The agent inside the session runs `git` and `gh` as ordinary subprocesses. Setting `GITHUB_TOKEN` and
the four `GIT_*` variables in the session's environment makes every one of them correct without
either tool learning anything about this stack.

⚠ **This is the decision most worth challenging.** An environment variable is visible to every process
in the session, including the agent, which can print it. The alternative — a credential helper that
hands a token to `git` per invocation and never to the agent — is stronger and considerably larger,
and it does not cover `gh` or the agent's own API calls, which *need* the token. Recorded so the
reviewer can weigh it; a helper can be added later without changing the resolution this node builds.

### What `tddy-tools` does

The PR tools in `tddy-tools/src/server.rs` are gated on `GITHUB_TOKEN` being set, and describe
themselves that way. After this node, in a session whose project has an account, it is set — to that
account's token. In a session whose project has none, the tools are absent, which is the correct
report: there is no account with which to open a PR.

## What's Staying the Same

- **WIP snapshot commits** stay `tddy-daemon <tddy-daemon@tddy.invalid>`, for the reason the code
  already records.
- **The REST client's call sites** — this node changes where the token comes from, not what any call
  does. After #492 they live in `tddy-github`.
- **Login, linking, the store, the screen, assignments, propagation** — 1/9 through 8/9 are used, not
  changed.
- **Projects with no account assigned** keep working for everything that is not a GitHub operation.

## Impact Analysis

| Package | Impact |
|---|---|
| `tddy-github` (**after #492**) | the REST client takes a resolved token instead of reading the environment |
| `tddy-accounts` | the resolver is called; name and email read from the record |
| `tddy-daemon-livekit` | the session's git environment carries the resolved identity |
| `tddy-daemon-auth` | `FileGitHubTokenStore`'s remaining readers move to the vault |
| `tddy-tools` | unchanged code; its PR tools become correctly scoped by consequence |

## Implementation Plan

1. The REST client accepts a token rather than resolving one (after #492, in `tddy-github`).
2. Resolve once at the session edge; construct the environment from the single answer.
3. Each `AccountResolution` outcome gets its own failure message.
4. Remove the environment-variable resolution path entirely.
5. Retire `FileGitHubTokenStore`'s remaining readers.
6. Acceptance: two projects, two accounts, one daemon, correct commits and pushes.

## Acceptance Criteria

- [ ] A commit made in a session carries the **assigned account's** name and email
- [ ] A GitHub API call from a session uses the **assigned account's** token
- [ ] Two projects on one daemon, assigned different accounts, act as different GitHub users
- [ ] The token and the identity always come from **one** resolution — never mixed
- [ ] `NotAssigned` fails GitHub operations with that reason; no environment fallback
- [ ] `UnknownOnThisHost` fails with *that* reason, distinct from `NotAssigned`
- [ ] `Ambiguous` fails rather than picking
- [ ] `GITHUB_TOKEN` in the **daemon's** environment is never used for a session
- [ ] WIP snapshot commits are still authored by `tddy-daemon`
- [ ] `FileGitHubTokenStore` has no readers left

## References

- [#492](https://github.com/uppin/tddy-coder/pull/492) — the move, and this stack's base
- `packages/tddy-workflow-recipes/docs/code-issues/squatting-github-rest-client.md` — 🚧 claimed by #492
- `packages/tddy-daemon-livekit/src/session_room.rs:337` — why WIP commits keep a machine identity
- `packages/tddy-workflow-recipes/src/github_rest_common.rs:17` — the environment resolution removed
