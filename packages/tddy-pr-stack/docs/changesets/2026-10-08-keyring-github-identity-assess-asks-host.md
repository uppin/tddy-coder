# 2026-10-08 — `AssessTask` asks the session host for its GitHub token

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

`AssessTask` builds `RealGithubPrApi::asking_the_session_host`, which asks the first time a node that owns a
branch has its PR looked up.

Detail: [architecture.md](../architecture.md#github-credential).
