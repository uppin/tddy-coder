# 2026-09-22 — Git plumbing becomes `tddy-git`, and the GitHub REST client moves to `tddy-github`

**Type:** Refactor

`#carve` 6/11, PR [#492](https://github.com/uppin/tddy-coder/pull/492).

Two bodies of low-level plumbing lived in crates that had nothing to do with them, and `#carve`
10/11 (`pr-stack-crate`, PR [#496](https://github.com/uppin/tddy-coder/pull/496)) consumes both.

**Git plumbing.** `tddy-core/src/worktree.rs` was 1,606 production lines of `git` wrappers that 35
dependent crates compiled whether they touched git or not, and only a handful of its functions knew
what a session was. The plumbing is now [`tddy-git`](../../../packages/tddy-git/README.md), a crate
that depends on **no workspace crate** — only `log`:

| Module | Production lines | Holds |
|---|---:|---|
| `refs` | 198 | branch and ref names, validation, slugs, free names |
| `rev` | 77 | `git rev-parse` wrappers |
| `remote` | 403 | `GIT_SSH_COMMAND`, fetch, push, default-remote resolution |
| `worktree` | 460 | linked worktrees: create, reuse, find, list, remove |
| `ssh_worktree` + `ssh_exec` | 70 + 96 | session worktrees on a remote host |

`tddy_core::worktree` keeps the four functions that read a changeset — **428** production lines — and
re-exports `tddy-git` with `pub use tddy_git::*;`; `tddy_core::ssh_exec` is a facade too.

**GitHub REST.** 2,124 lines of it lived in `tddy-workflow-recipes`, while
[`tddy-github`](../../../packages/tddy-github/README.md) had no PR surface at all.
`github_rest_common`, `github_pr` and `orchestrate_pr_stack/github.rs` — now `tddy_github::pr_api` —
moved leaf-first, and the origin keeps three one-line facades.

**No consumer was edited.** `tddy-tools/src/server.rs` and every `tddy-core` dependent compile
untouched; every old path resolves. Nothing changes behaviour: all four whole-file moves are
byte-identical under `git show HEAD:<old> | diff - <new>`, and the two splits differ from their
sources only by visibility widenings (`pub` × 5 across the crate boundary, `pub(crate)` × 3 across
the new modules), `use` lines, module wiring and four doc-link targets.

## Three premises the plan got wrong

1. **`orchestrate_pr_stack/github.rs` does name `tddy_core`.** It states `tddy_core::WorkflowError`
   in 16 production signatures, including every method of the public `GithubPrApi` trait, fully
   qualified inline where a `use` / `crate::` grep does not see them. `tddy-github` therefore depends
   on `tddy-core` — a developer decision; it closes no cycle.
2. **A 450-line target could not hold with the five functions the plan kept in `tddy-core`** — they
   were 478 lines on their own. `setup_worktree_for_session_over_ssh`, which never reads a changeset,
   moved to `tddy-git` as well.
3. **"No cycle is possible" checked only `tddy-github`'s outgoing edges.** The facade direction
   closes `recipes → github → service → recipes`, and cargo builds nothing. `tddy-service`'s
   `tddy-workflow-recipes` dependency is now a **dev-dependency**: its only three uses are inside a
   `#[cfg(test)]` module.

Every phase the plan called mechanical was done by hand. `restructure anchors` resolved no item, and
`move_module_to_crate` refuses any move where a file left behind names the moved module — which is the
facade shape both halves needed.

## Code issues closed

Both were claimed by this PR and re-measured clean at wrap; their files are deleted.

| Record | At detection (2026-09-15) | At wrap (2026-09-22) |
|---|---|---|
| `tddy-core` — `squatting-git-plumbing-worktree` | `worktree.rs` 1,606 production lines · 48 functions · git plumbing locked in the god-crate | 428 production lines · 4 functions, all of which read a changeset · `tddy-git` has 0 workspace dependencies |
| `tddy-workflow-recipes` — `squatting-github-rest-client` | 2,124 lines of GitHub REST across 3 files | 9 lines — three one-line facades over `tddy_github` |

Both records also carried the two false premises above; neither survives into the code.

## Code issues opened

- `tddy-github` — `oversized-file-pr-api`: `pr_api.rs` is **≈ 915** real production lines, but the
  budget gate reports 315 because production code resumes after its first test module. Inherited
  byte-identical from the move.
- `tddy-github` — `dead-code-github-pr-mock-transport`: `MockGithubTransport` and the
  `create_pull_request` / `update_pull_request` pair taking it are compiled into production with no
  production caller.
- `tddy-workflow-recipes` — `misplaced-tests-github-rest-client`: three suites import only the
  facades, so they test `tddy-github` code from the wrong crate.

`tddy-core`'s two `complexity-worktree-*` records gain an **unchanged** row: the functions they
describe stayed, byte-identical.

## Backlog

No `docs/dev/todo/` entry is resolved here. `2026-08-13-pr-stack-an-externally-located-worktree-is-refused-as-a-stack-base`
was re-aimed at `tddy-git` — the resolver it describes moved, unchanged. One entry was added:
`2026-09-20-github-pr-log-targets-name-the-crate-the-code-left` — 13 log statements in
`tddy_github::github_pr` still carry `target: "tddy_workflow_recipes::github_pr"`, since retargeting
them would break any log filter on the old target.

## Verification

Scoped to the packages touched: `cargo test -p tddy-core -p tddy-git -p tddy-github
-p tddy-workflow-recipes --no-fail-fast` — 1,205 passed, 1 failed, and the failure is pre-existing
and environmental (`pr_stack_artifact_paths_acceptance` compares an un-canonicalised `/tmp` against
macOS's `/private/tmp`). The shape suite `tddy-github/tests/git_plumbing_shape.rs` pins the result:
`tddy-git` depends on no workspace crate and names no `Changeset`; `worktree.rs` is under 450
production lines and keeps the session layer; `tddy-github` declares all three moved modules and
never depends on `tddy-workflow-recipes`.
