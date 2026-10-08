# 2026-10-08 — `session_git_environment` renders the four commit-identity pairs

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

`session_git_environment(&ActingIdentity)` (`src/session_git.rs`) returns the four `GIT_AUTHOR_*` /
`GIT_COMMITTER_*` pairs, token excluded. New path dependency on `tddy-accounts` (no external dependency, no
cycle). WIP snapshot identity is unchanged.

Detail: [session-room.md](../session-room.md#the-sessions-git-identity).
