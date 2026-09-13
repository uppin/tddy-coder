# WorktreeService (tddy-worktree-service)

Everything about the git worktrees a daemon serves: listing them per project, sizing them, cleaning
and restoring them, and browsing and reading the files inside one — served as
`worktree.WorktreeService`.

## Where the code lives

| Group | Files | What is in them |
|---|---|---|
| `service.rs` | 1 | `WorktreeServiceImpl` — the nine handlers, the state they read, and the `with_*` builders |
| `stream.rs` | 1 | `MpscWorktreeStatsStream`, `MpscResultStream`, `worktree_file_frames` — the streaming adapters |
| the worktrees | `worktrees`, `worktree_files` | `git worktree list` parsing, the stats cache, path validation against a repo root, and the code-pane file gate |
| the projects | `project_storage`, `project_provision` | the per-daemon `projects.yaml` registry and the provisioning a session start needs |
| branches | `branch_owner`, `branch_intent`, `base_sync_cache` | who owns a branch, what a caller meant by one, and the memoised base comparison |
| `remote_git_service` | 1 | every project served as a git remote over any tddy-rpc transport |
| `test_util.rs` | 1 | shared helpers, ungated, because the `tests/` suites reach for them |

## Methods

| RPC | Purpose |
|---|---|
| `ListWorktreesForProject` | Cached rows for a project (`refresh` runs `WorktreeStatsCache::refresh_stats_for_project` on a blocking worker). Overlays each row's size status and timestamp while keeping the eager `disk_bytes` walk as a cache fallback |
| `RemoveWorktree` | `remove_worktree_under_repo`, then `invalidate_project`. Its preamble — token → GitHub user → mapped OS user → project → `main_repo_path_for_host` — is the one the file RPCs reuse |
| `StreamWorktreeStats` | Lazy, semaphore-bounded per-worktree disk usage: a full snapshot frame, then one frame per worktree as its size flips `Calculating → Cached` |
| `CalculateWorktreeSize` | (Re)triggers one worktree, membership-gated; the result surfaces on any open stream rather than in the reply |
| `CleanWorktree` | `git clean -fdx` in a **secondary** session worktree. The primary (first-listed) worktree is refused |
| `RestoreSessionWorktree` | Recreates a missing session worktree from its persisted changeset, reusing `tddy-core`'s session-worktree setup. It keeps suffixing on a branch conflict — only a surface that can prompt an operator asks to be rejected |
| `ListWorktreeDirectory` | One directory level at `rel_path` (empty = root), `.gitignore`-aware and `.git`-excluded |
| `ReadWorktreeFile` | UTF-8 read of a path the listing surfaced, capped at `MAX_WORKTREE_FILE_BYTES` (1 MiB) with a `truncated` flag and the full `byte_size` |
| `StreamReadWorktreeFile` | The streaming, byte-exact sibling: same request, same resolution, same guards, but `bytes` rather than a string — so a PNG or a file in any encoding round-trips intact — and an oversized file is **refused before the first frame** rather than truncated, because a truncated mirror is a wrong mirror |

## The code-pane file gate

Browsing a session's **worktree** (the git checkout at `SessionEntry.repo_path`) is not browsing its
metadata directory — it powers the web [Code pane](../../../docs/ft/web/session-code-pane.md).

- **Implementation**: filesystem and git policy live in `worktree_files`. `ListWorktreeDirectory` and
  `ReadWorktreeFile` reuse the `RemoveWorktree` preamble, then gate on
  `worktrees::worktree_path_is_listed` so the `worktree_path` must appear in the project's
  `git worktree list`. The git/fs work runs inside `spawn_blocking` with
  `spawn_worker_request_timeout()`.
- **Listing**: `git ls-files --cached --others --exclude-standard -z`; a linked worktree's private
  `<gitdir>/info/exclude` is fed in explicitly via `--exclude-from`, because git treats `info/` as
  shared and `--exclude-standard` alone would miss it. Entries are directories-first then files, each
  alphabetical.
- **Reading**: refuses any path the listing did not surface — so `.git` and ignored files, `.env`
  among them, cannot be read — and applies traversal rejection (`..`/absolute) plus
  canonicalize-and-contain under the worktree root.

**Agent context files are a separate reader, deliberately.** `tddy-daemon`'s `context_files` serves
a session's agent configuration, which is routinely gitignored (`.claude/settings.local.json`,
`**/.cursor/mcp.json`), so git's listing cannot be its gate. The two share only the traversal and
containment guards — `validate_rel_path_shape` and `canonicalize_root`, which are `pub` here because
that caller stayed behind — and **never share a gate**.

## `branch_owner` takes a port rather than knowing what a session is

A branch belongs to a worktree; what *claims* one is a session, and sessions stay in `tddy-daemon`.
So `find_session_owning_branch(listing, sessions_base, branch)` reads sessions through a
`SessionListing` port, and what crosses it is a `SessionClaim` — `session_id`, `is_active`, `status`,
`updated_at`, the four fields the ownership rule judges on — rather than everything a session is. A
port that carried more would make every later field of a session part of this crate's contract.
`tddy-daemon`'s `session_reader::DaemonSessionListing` is the implementation.

The rule itself is unchanged and still has one home: scan for a `Changeset.branch` match, prefer an
**active** session, tie-break on the most recent `updated_at`, skip a session whose changeset cannot
be read. It is synchronous, so async callers wrap it in `spawn_blocking_with_timeout`.

## How it is served

`tddy-daemon`'s `runtime.rs` builds one `WorktreeServiceImpl` and registers it from the same `Arc`
on all three transports: a `ServiceEntry` from `tddy_service::WorktreeServiceServer::from_arc` for
Connect-HTTP `/rpc` and for the LiveKit common room, and `WorktreeServiceTonicAdapter` on the local
UDS socket, added to the same `Server::builder()` as `ConnectionService`.

**No method here routes to a peer.** A worktree is a directory on the daemon that holds it, and no
request in this service carries a `daemon_instance_id` to route by — which is what makes this
service's wiring simpler than `host.HostService`'s.

`WorktreeRoomCloser` is the seam back to the daemon's LiveKit rooms — a worktree that is removed or
restored may need its session's room closed, and `NoSessionRooms` is the no-op a deployment without
rooms uses.

## Testing

| Level | Where |
|---|---|
| Unit | each module's own `#[cfg(test)]` — path validation, the stats cache, the branch-ownership rule |
| Integration | `tests/worktrees_rpc.rs`, `worktrees_acceptance.rs`, `worktree_files_rpc.rs`, `stream_worktree_stats_rpc.rs`, `stream_read_worktree_file_acceptance.rs`, `stream_read_worktree_file_rpc_acceptance.rs`, `worktree_session_actions_acceptance.rs` |
| Component | `packages/tddy-web/cypress/component/WorktreesAppPage.cy.tsx` and the session-inspector suites |

## See also

- [worktrees.md](./worktrees.md) — the library helpers: `git worktree list` parsing, the stats
  snapshots, path validation and removal
- [remote-git-service.md](./remote-git-service.md) — every project as a git remote over tddy-rpc
- Sibling service: [`tddy-host-service`](../../tddy-host-service/docs/host-service.md)
- What stayed: [connection-service.md](../../tddy-daemon/docs/connection-service.md)
- Feature: [docs/ft/web/worktrees.md](../../../docs/ft/web/worktrees.md),
  [docs/ft/web/worktree-disk-usage-streaming.md](../../../docs/ft/web/worktree-disk-usage-streaming.md),
  [docs/ft/web/session-worktree-inspector.md](../../../docs/ft/web/session-worktree-inspector.md)
- [changesets/](./changesets/)
