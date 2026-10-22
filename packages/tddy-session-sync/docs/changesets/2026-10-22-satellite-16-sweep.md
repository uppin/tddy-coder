# 2026-10-22 — attach and sync split into topic modules

**Type:** Refactor

`#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)); cross-package entry:
[2026-10-22-satellite-16-sweep.md](../../../../docs/dev/changesets/2026-10-22-satellite-16-sweep.md). Behaviour-preserving unless a line says otherwise; production lines are counted to the first
`#[cfg(test)]`.

- `src/attach.rs` **555 → 371**: the daemon's Connect-HTTP calls to `attach/daemon_http.rs` (195).
- `src/sync.rs` **788 → 454**: the git it issues to `sync/git.rs` (141), the record/worktree decisions,
  the delta request and reassembly to `sync/delta.rs` (215), re-exported from `sync`.
- By `move_item`; no path a caller names changed. Records closed and deleted: `oversized-file-attach.md`
  (555 → 371) and `oversized-file-sync.md` (788 → 454).
