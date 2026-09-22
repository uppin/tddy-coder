# 2026-09-22 — `tddy-workflow-recipes` becomes a dev-dependency

**Type:** Refactor · `#carve` 6/11, PR [#492](https://github.com/uppin/tddy-coder/pull/492)
Cross-package entry: [`docs/dev/changesets/2026-09-22-carve-git-plumbing.md`](../../../../docs/dev/changesets/2026-09-22-carve-git-plumbing.md)

`tddy-github` depends on this crate for its generated `proto::auth` types, and
`tddy-workflow-recipes` now depends on `tddy-github` for its GitHub REST facades — so a production
edge from here to `tddy-workflow-recipes` would be a package cycle. It was never a production edge
in use: the only three references are `use tddy_workflow_recipes::TddRecipe;` inside
`src/integration_tests.rs`, which `lib.rs` declares `#[cfg(test)]`. It is a dev-dependency now, which
cargo permits a cycle through. No code changed.
