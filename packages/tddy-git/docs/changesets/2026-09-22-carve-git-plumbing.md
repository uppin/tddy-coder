# 2026-09-22 — The crate is created from `tddy-core`'s git plumbing

**Type:** Refactor · `#carve` 6/11, PR [#492](https://github.com/uppin/tddy-coder/pull/492)
Cross-package entry: [`docs/dev/changesets/2026-09-22-carve-git-plumbing.md`](../../../../docs/dev/changesets/2026-09-22-carve-git-plumbing.md)

Created from the pure-git portion of `tddy-core/src/worktree.rs` (1,606 production lines), plus
`ssh_exec` and `setup_worktree_for_session_over_ssh`, which take a session id as a string and never
read a changeset. It depends on **no workspace crate** — only `log` — and must not gain one.

Five private modules re-exported at the root, so every item is `tddy_git::<item>`: `refs` (198
production lines), `rev` (77), `remote` (403), `worktree` (460), `ssh_worktree` (70), with
`ssh_exec` (96) public. Every moved body is byte-identical; five helpers the session layer calls
across the crate boundary became `pub`, and three `rev` helpers `pub(crate)`. Its 17 tests came
with the code, byte-identical. See [architecture.md](../architecture.md).
