# 2026-09-09 — `worktree.WorktreeService` in its own crate

**Type:** Architecture

New crate, added by the root node of the `#unbundle` stack
([#470](https://github.com/uppin/tddy-coder/pull/470)). Full story in the cross-package entry:
[2026-09-09-unbundle-host-worktree-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-host-worktree-services.md).

8 modules left `tddy-daemon` and now serve nine methods as `worktree.WorktreeService`:
`ListWorktreesForProject`, `RemoveWorktree`, `StreamWorktreeStats`, `CalculateWorktreeSize`,
`CleanWorktree`, `RestoreSessionWorktree`, `ListWorktreeDirectory`, `ReadWorktreeFile`,
`StreamReadWorktreeFile`. Surface and wiring: [worktree-service.md](../worktree-service.md).

**No method here routes to a peer.** A worktree is a directory on the daemon that holds it, and no
request in this service carries a `daemon_instance_id` to route by — which is what makes this
service's wiring simpler than the host service's.

**`branch_owner` takes a port rather than carrying `session_reader` with it.** A branch belongs to a
worktree, which is this crate; what *claims* one is a session, which stays in `tddy-daemon`. The seam
is `branch_owner::SessionListing`, and what crosses it is `SessionClaim` — the four fields the
ownership rule judges on — rather than everything a session is. A port that carried more would make
every later field of a session part of this crate's contract.
`tddy_daemon::session_reader::DaemonSessionListing` is the implementation.

Three `worktree_files` helpers widened from `pub(crate)` to `pub` for callers that stayed behind:
`canonicalize_root` and `validate_rel_path_shape` for `context_files`/`context_sync`, and
`git_listed_files` for `host_documents`. The two readers still **never share a gate** — agent config
is routinely gitignored, so git's listing cannot serve it — only the traversal and containment
guards.

Seven integration suites moved with the code. 81 passing.

Moved with the code: [worktrees.md](../worktrees.md),
[remote-git-service.md](../remote-git-service.md).
