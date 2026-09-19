# 2026-09-20 — tddy-github — `github_pr` log targets name the crate the code left

**Category:** Correctness — diagnostics
**Source:** `#carve` 6/11 (`carve-git-plumbing` changeset, PR #492)

`packages/tddy-github/src/github_pr.rs` carries **13** log statements whose explicit target is the
crate the file no longer lives in:

```rust
log::debug!(
    target: "tddy_workflow_recipes::github_pr",
    ...
);
```

First at `github_pr.rs:119`; `grep -c 'target: "tddy_workflow_recipes' packages/tddy-github/src/github_pr.rs`
counts them. No other file in `tddy-github` has this.

`#carve` 6/11 moved the file from `tddy-workflow-recipes` to `tddy-github` as a **pure relocation** —
its boundary forbids changing any log message, and the moved body is byte-identical to its origin.
Retargeting these is therefore out of that node's scope, and deliberately so: the target string is
not cosmetic. Anything filtering logs by `tddy_workflow_recipes::github_pr` — an `RUST_LOG` directive,
a log-collection rule — keeps working today and would stop the moment the target changes. That is a
real, if small, operational break, and it wants its own commit and its own announcement rather than
riding along inside a refactor.

**The fix** is a single find-and-replace of `tddy_workflow_recipes::github_pr` →
`tddy_github::github_pr` across those 13 sites, in a commit that does nothing else, after checking
for consumers of the old target:

```bash
grep -rn 'tddy_workflow_recipes::github_pr' --include='*.rs' --include='*.toml' --include='*.yaml' --include='*.sh' .
```

Until then the targets are stale but harmless — they misattribute the emitting crate in logs.
