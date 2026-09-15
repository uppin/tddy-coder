# Changeset: carve-git-plumbing

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Refactor
**Stack**: `#carve` 5/9
**PR**: [#492](https://github.com/uppin/tddy-coder/pull/492)

PRD: [`2026-09-15-carve-git-plumbing-prd.md`](./2026-09-15-carve-git-plumbing-prd.md)

## Initial Discovery

[`2026-09-15-carve-git-plumbing-initial-discovery.md`](./2026-09-15-carve-git-plumbing-initial-discovery.md)

## Affected Packages

- **`tddy-git`** (new): the pure-git portion of `tddy-core/src/worktree.rs`.
- **`tddy-core`**: [README.md](../../../packages/tddy-core/README.md) — `worktree.rs` keeps only the
  session-aware layer, behind a facade.
- **`tddy-github`**: [README.md](../../../packages/tddy-github/README.md) — gains the PR REST surface.
- **`tddy-workflow-recipes`**: [README.md](../../../packages/tddy-workflow-recipes/README.md) — loses
  2,124 lines of GitHub REST, keeps facades.

## Responsibility

- Create `tddy-git` and move ~1,200 production lines of pure git plumbing into it.
- Leave `tddy-core::worktree` as the session-aware layer plus a facade.
- Move `github_rest_common.rs`, `github_pr.rs` and `orchestrate_pr_stack/github.rs` into
  `tddy-github`, leaf-first, behind facades.

## Boundaries

- Does **not** touch the PR-stack data model — `pr_stack/` and
  `orchestrate_pr_stack/{git_ops,assess,pr_insight,actions}` are `#carve` 9/9's.
- Does **not** touch `changeset.rs` — `#carve` 4/9's.
- Does **not** edit any consumer. `tddy-tools/src/server.rs` and all 35 `tddy-core` dependents keep
  compiling unchanged.
- Does **not** change git invocation, retry behaviour or error text.
- Does **not** rely on `#carve` 3/9's cluster support — see `## Dependencies`.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `1/9` restructure-moves | nested anchors accepted; facade-aware cycle refusal | `orchestrate_pr_stack/github.rs` is **nested**; both moves leave `pub use` facades | touch `tddy-code-restructuring` |
| `3/9` restructure-clusters | cluster moves; plan-scoped journal | **not consumed as behaviour.** The three GitHub files are a DAG (`github_pr → github_rest_common ← orchestrate::github`), so leaf-first ordering makes each rewrite correct when made; `worktree` is one module after Phase A | rely on cluster moves |
| `4/9` core-foundations | `changeset/` split, `tddy-workflow` vocabulary | **not consumed.** `worktree.rs` keeps its `crate::changeset` use; this node does not move it | move or edit `changeset.rs` |

> **Sequencing note, not a licence:** based on 4/9 because the stack is a line. Its only real
> predecessor is 1/9.

## Draft PR contract

Published first:

**Published** (commit 2): `packages/tddy-github/tests/git_plumbing_shape.rs` — five assertions
pinning both halves. `tddy-git` does not exist yet, and creating a crate skeleton is Phase B
implementation rather than surface, so what lands first is the **contract the crate must satisfy**:
it exists, depends on no workspace crate, and holds nothing that names `Changeset`.

The four helpers `#carve` 9/9 names — `detect_default_remote_name`, `worktree_path_for_branch`,
`local_branch_name_for_remote`, `checked_out_branch_name` — are existing `tddy-core::worktree`
functions that keep their signatures through the move, so there is no new signature to declare.

This PR goes on to implement all of it. **It must not merge in that state.**

## Green wave

**Wave:** 2 of 4
**Greenable independently:** **no** — one nested anchor and two facades put both of `#carve` 1/9's
fixes on this node's path; they must exist as behaviour first
**Concurrent with:** `#carve` 3/9, 4/9
**Blocks:** 9/9 `pr-stack-crate`, which needs four `tddy-git` helpers and the GitHub client

Real dependency edges, as opposed to the branch line:

    n1 → n3, n4, n5, n6, n9      n3 → n6, n7, n9      n4 → n8, n9      n5 → n9

## Prerequisites

| Entry | Verdict | What this node does with it |
|---|---|---|
| [2026-07-04-tddy-github-tddy-daemon.md](../todo/2026-07-04-tddy-github-tddy-daemon.md) | ⚠ **DURING** | Names `tddy-github`'s surface and its relationship to the daemon. Read before widening that crate; this node must not contradict it. Not claimed. |
| [2026-08-13-pr-stack-an-externally-located-worktree-is-refused-as-a-stack-base.md](../todo/2026-08-13-pr-stack-an-externally-located-worktree-is-refused-as-a-stack-base.md) | ⚠ **DURING** | Concerns worktree resolution this node relocates. The behaviour must survive the move **unchanged** — including the refusal. Not claimed; fixing it is a behaviour change and this node has none. |
| [2026-07-30-pr-stack-full-control-follow-ups.md](../todo/2026-07-30-pr-stack-full-control-follow-ups.md) | — Unrelated | Concerns the PR-stack product surface, not this plumbing. |

## State A → State B

### State A

- `tddy-core/src/worktree.rs` — 1,606 prod lines, 48 free functions. Its only `crate::changeset`
  import (`worktree.rs:11`) is consumed at 828–962 and 1178–1273, inside exactly **two** functions.
  35 crates compile all of it.
- `tddy-workflow-recipes` holds 2,124 lines of GitHub REST across three files:
  `github_rest_common.rs` (338, **zero deps**), `github_pr.rs` (494, depends only on the former),
  `orchestrate_pr_stack/github.rs` (1,292, depends only on `github_token_from_env`).
- `tddy-github` is 1,340 lines of OAuth/tokens with **no PR surface**, and depends only on
  `tddy-rpc` and `tddy-service`. `tddy-workflow-recipes` does not depend on it.
- `tddy-tools/src/server.rs` consumes `github_pr` from across the crate boundary.

### State B

- `tddy-git` holds the plumbing and depends on no `tddy-*` crate.
- `tddy-core::worktree` is the session-aware layer plus a facade, under 450 prod lines.
- `tddy-github` owns the PR REST surface; `tddy-workflow-recipes` keeps facades.

## Implementation phases

| Phase | Kind | Work |
|---|---|---|
| **A** | mechanical | `extract_module --to_file` on `worktree.rs`: lift the pure-git functions into a **flat** `git_plumbing` module, leaving the two session-aware functions behind. Flat is what `move_module_to_crate` requires |
| **B** | manual | `tddy-git`'s `Cargo.toml` + `lib.rs`, and the workspace `members` entry — **no `create_file` operation exists**, by design |
| **C** | mechanical | `move_module_to_crate` on `git_plumbing` → `tddy-git`, `reexport: "glob"` |
| **D** | mechanical | `move_module_to_crate` × 3 → `tddy-github`, **leaf-first**: `github_rest_common`, `github_pr`, then the nested `orchestrate_pr_stack/github.rs`. Separate plan — `.restructure/` is repo-scoped until 3/9 lands |
| **E** | manual | `Cargo.toml` dependency edges; facade tidy-up; `README.md` × 3 |

The ordering in Phase D is the whole reason this node does not need 3/9: **leaf-first turns a
three-module move into three correct single-module moves.**

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [x] Publish the draft-PR contract (`tddy-git` + `tddy-github` surfaces, failing tests)
- [x] Failing acceptance tests — **USER REVIEW** (approved 2026-09-15, gates delegated)
  - `packages/tddy-github/tests/git_plumbing_shape.rs` — 4 failing (`tddy-git` absent, `worktree.rs`
    still 1,606 prod lines, `tddy-github` publishes no PR surface); **1 passing**:
    `tddy_github_gains_no_dependency_on_the_crate_the_client_left` is the cycle guard and must stay
    green through the move.
- [x] Failing unit/integration tests — the same suite; "which crate owns this" is not a question the type system answers once everything compiles
- [ ] Implement production code making tests pass (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

```bash
./test -p tddy-core -p tddy-git -p tddy-github -p tddy-workflow-recipes
cargo clippy -p tddy-core -p tddy-git -p tddy-github -p tddy-workflow-recipes -- -D warnings
cargo build -p tddy-tools     # proves AC5 — server.rs was not edited
cargo fmt --all --check
tddy-tools restructure verify --against HEAD
```
