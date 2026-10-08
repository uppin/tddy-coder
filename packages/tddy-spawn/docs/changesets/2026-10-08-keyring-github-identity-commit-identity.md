# 2026-10-08 — Commit identity pairs ride the spawn wire; `GITHUB_TOKEN` is not inherited

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

`SpawnRequest::git_environment` (`#[serde(default)]`), `SpawnOptions::git_environment` and
`SessionChildPlan::env` carry the four pairs through all three backends. `plan_session_child` refuses any
other key. `spawn_as_user` removes `GITHUB_TOKEN` and `GH_TOKEN` from the child. The package gains its first
`docs/` directory.

Detail: [commit-identity.md](../commit-identity.md).
