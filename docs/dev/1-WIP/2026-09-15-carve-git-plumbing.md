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

### State B — as delivered

- **`tddy-git`** (new): 1,763 lines — the plumbing plus `ssh_exec`, depending on nothing but `log`.
- **`tddy-core::worktree`**: **428 production lines** (was 1,606) — four session-aware functions
  plus `pub use tddy_git::*;`. `tddy-core::ssh_exec` is a three-line facade.
- **`tddy-github`**: owns `github_rest_common`, `github_pr` and `pr_api`
  (ex-`orchestrate_pr_stack/github.rs`). Depends on `tddy-core` — a developer decision, see below.
- **`tddy-workflow-recipes`**: three one-line facades; 2,124 lines gone.
- **`tddy-service`**: `tddy-workflow-recipes` demoted to a **dev-dependency** to break the package
  cycle the facade edge closes. Test-only usage, so no production path changed.

## Implementation phases

| Phase | Planned kind | What green actually did |
|---|---|---|
| **A** | mechanical | **By hand.** `extract_module` needs one contiguous anchor range, and `restructure anchors` resolves no item at all right now |
| **B** | manual | As planned: `tddy-git`'s `Cargo.toml` + `lib.rs` + the workspace `members` entry |
| **C** | mechanical | **By hand.** `move_module_to_crate` refuses any move where a file left behind names the moved module — exactly the facade shape required here |
| **D** | mechanical | **By hand**, `git mv` × 3, leaf-first as planned. The leaf-first ordering did pay off: `crate::github_rest_common::…` resolves unchanged once all three sit in one crate, so **not one path inside the moved bodies was rewritten** |
| **E** | manual | As planned, plus the two manifest edits the plan did not foresee (`tddy-core` on `tddy-github`, and `tddy-service`'s dev-dependency demotion) |

**Every phase the plan called mechanical was done by hand.** That is not a shortcut — it is the
third node in this stack to find the restructure operations inapplicable to a facade-leaving
cross-crate move. Identity was proved with `diff` instead: see `## Verification`.

The leaf-first ordering in Phase D was still the right call for the reason the plan gave — it turned
a three-module move into three independent ones.

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
- [x] Implement production code making tests pass (`/green`) — **5/5 shape tests green**
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Decisions taken during green

Three of this changeset's premises did not survive contact with the code. The first two were put to
the developer rather than resolved by whichever reading kept a test green. The third had only one
available answer and was taken without asking — it is called out as such below.

**1. `orchestrate_pr_stack/github.rs` does name `tddy_core`.** The discovery's "none of them touches
`tddy_core`" came from a `use` / `crate::` grep; the 16 references are fully qualified inline, and
they include every method of the public `GithubPrApi` trait. **Developer chose:** move all three
files and take the `tddy-github → tddy-core` edge. Consequently the AC6 guard test was **narrowed**
to forbid only `tddy-workflow-recipes` — the crate the client left, and the only edge that can close
a cycle. The `tddy-core` clause rested on the false premise and was removed, with the reasoning
written into the test's doc comment. This is the one test change in this PR.

**2. AC2's 450-line threshold is incompatible with FR1's list of stayers.** The five functions FR1
named are 478 production lines on their own; the floor was 488. **Developer chose:** move
`setup_worktree_for_session_over_ssh` and `ssh_exec.rs` to `tddy-git` as well — it is the one item
on the list that never reads a `Changeset`. Result: 428 lines, AC2 met as written, FR1 corrected.

**3. "`tddy-github` depends only on `tddy-rpc` and `tddy-service`, so no cycle is possible" was
wrong.** It checked only `tddy-github`'s outgoing edges. The facade direction closes
`recipes → github → service → recipes` and `cargo` refuses to build the workspace. Inherent to FR2 —
it would have fired even if only the two leaf files moved. Resolved by demoting `tddy-service`'s
`tddy-workflow-recipes` dependency to a dev-dependency (its only three uses are inside a
`#[cfg(test)] mod`). **This is the one edit outside the changeset's stated package set**, and it was
**not** put to the developer: with the cycle in place `cargo` builds nothing at all, so there was no
state in which to ask. The alternative — cutting `tddy-github → tddy-service` by relocating the
`proto::auth` types — is far larger and touches a crate nobody scoped. Worth a look at review.

## Deferred

- `packages/tddy-github/src/github_pr.rs` carries **13** log statements with
  `target: "tddy_workflow_recipes::github_pr"`, now naming a crate the code has left. Retargeting
  them is a log-message change, which this node's boundary forbids, and it would break any log
  filter configured against the old target. Recorded in
  [`docs/dev/todo/2026-09-20-github-pr-log-targets-name-the-crate-the-code-left.md`](../todo/2026-09-20-github-pr-log-targets-name-the-crate-the-code-left.md).

## Verification

```bash
cargo test -p tddy-core -p tddy-git -p tddy-github -p tddy-workflow-recipes --no-fail-fast
cargo clippy -p tddy-core -p tddy-git -p tddy-github -p tddy-workflow-recipes --all-targets -- -D warnings
cargo build -p tddy-tools && cargo check -p tddy-tools --all-targets   # AC5 — server.rs not edited
cargo check -p tddy-service --all-targets                             # covers the manifest demotion
cargo fmt --all --check
```

`--no-fail-fast` is not optional: `./test` does not pass it, and one red suite hides every target
after it in these crates.

**AC7 — identity, proved by `diff` rather than `restructure verify`.** No restructure plan was run
(see `## Implementation phases`), so `restructure verify --against HEAD` has nothing to compare.

```bash
# all four whole-file moves: empty output
for p in \
  packages/tddy-workflow-recipes/src/github_rest_common.rs:packages/tddy-github/src/github_rest_common.rs \
  packages/tddy-workflow-recipes/src/github_pr.rs:packages/tddy-github/src/github_pr.rs \
  packages/tddy-workflow-recipes/src/orchestrate_pr_stack/github.rs:packages/tddy-github/src/pr_api.rs \
  packages/tddy-core/src/ssh_exec.rs:packages/tddy-git/src/ssh_exec.rs; do
  git show "HEAD:${p%%:*}" | diff - "${p##*:}"
done

# the worktree.rs split: line-multiset diff shows only the 5 `pub fn` widenings + doc/facade lines
git show HEAD:packages/tddy-core/src/worktree.rs | grep -v '^[[:space:]]*$' | sort > /tmp/old
cat packages/tddy-core/src/worktree.rs packages/tddy-git/src/lib.rs | grep -v '^[[:space:]]*$' | sort > /tmp/new
comm -3 /tmp/old /tmp/new
```
