# tddy-git

Pure `git` plumbing: worktrees, branches, refs and remotes. Every function here shells out to the
`git` command line, and **none of them knows what a session is** — no `Changeset`, no workflow, no
project registry.

That is the whole point of the crate. This code spent its life inside `tddy-core/src/worktree.rs`,
where 35 dependent crates compiled 1,606 lines of git plumbing whether they touched git or not, and
two session-aware functions were enough to bind all of it to the god-crate. Splitting the two apart
leaves a dependency-free library anything in the workspace can use.

**`tddy-git` depends on no other workspace crate**, and must not gain one. Its only dependency is
`log`. A `tddy-git` that reached back into `tddy-core` would have moved the lines without moving the
coupling — `packages/tddy-github/tests/git_plumbing_shape.rs` asserts against exactly that.

## Module layout

| Module | Owns |
|---|---|
| `refs` | branch and ref names: local-name derivation, ref validation, slugs, free branch names |
| `rev` | `git rev-parse` wrappers: resolved revisions and checked-out branch names |
| `remote` | `GIT_SSH_COMMAND`, fetch, push, default-remote detection, integration-base resolution |
| `worktree` | linked worktrees under `.worktrees/`: create, reuse, find, list, remove |
| `ssh_worktree`, `ssh_exec` | session worktrees on a remote host, and the OpenSSH helpers they run through |

The modules are private and re-exported at the root, so every item is `tddy_git::<item>`. Details
per function: [docs/architecture.md](docs/architecture.md).

`setup_worktree_for_session_over_ssh` lives here rather than in `tddy-core` despite its name: it is
`git clone` plus `git worktree add` over SSH, parameterised by a session-id **string**. It never
reads a changeset.

## What stayed in `tddy-core`

`tddy_core::worktree` keeps the four functions that genuinely read a `Changeset` —
`setup_worktree_for_session`, `setup_worktree_for_session_with_integration_base`,
`setup_worktree_for_session_with_optional_chain_base` and
`resolve_persisted_worktree_integration_base_for_session` — and re-exports this crate with
`pub use tddy_git::*;`. `tddy_core::ssh_exec` is a facade over `tddy_git::ssh_exec`.

**No public path changed.** Every pre-existing `tddy_core::worktree::…` and `tddy_core::ssh_exec::…`
path still resolves, and no consumer was edited. Write new code against `tddy_git` directly.

## Error convention

Every fallible function returns `Result<_, String>`, carrying the `git` invocation's own stderr.
That is the convention the code arrived with and it is deliberately unchanged — a typed error would
be a behaviour change to 35 dependents.
