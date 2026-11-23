# WorktreeService (tddy-worktree-service)

Everything about the git worktrees a daemon serves: listing them per project, sizing them, cleaning
and restoring them, and browsing and reading the files inside one — served as
`worktree.WorktreeService`.

## Where the code lives

| Group | Files | What is in them |
|---|---|---|
| `service.rs`, `service/inspector.rs` | 2 | `WorktreeServiceImpl` — the handlers, the state they read, and the `with_*` builders; the session inspector's sync and PR-stream handlers are in the child module `inspector` |
| the inspector | `worktree_sync`, `pull_request_view` | a session worktree's sync state, ff-only pull and plain push; a session's PR view read through the GitHub inbox |
| `stream.rs` | 1 | `MpscWorktreeStatsStream`, `MpscResultStream`, `worktree_file_frames` — the streaming adapters. `MpscResultStream::into_receiver` hands back the channel a stream wraps. `tddy-session-lifecycle` re-exports the type at `connection_service::MpscResultStream` rather than defining its own |
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
| `GetWorktreeSyncState` | A session worktree's dirty files and its branch's relation to its upstream (`worktree_sync::read_sync_state`). A target session answers `unavailable_reason` |
| `PullWorktree` / `PushWorktree` | `git merge --ff-only <upstream>` / `git push` (`-u origin <branch>` when unpublished, never forced), each answering the state after it |
| `StreamSessionPullRequest` | The PR view of the session's branch, then a new one on every inbox event that changes it (`pull_request_view::watch_pull_request`) |
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

**`resolve_listed_worktree` is the authorisation other services reuse.** It is `pub` on
`WorktreeServiceImpl`: it resolves the token to an OS user, the project to its main repo on this host,
and returns the worktree root only when the path appears in that repo's `git worktree list`.
`tddy-daemon`'s [code navigation service](../../tddy-daemon/docs/code-navigation-service.md) calls it, so
go-to-definition can never reach a worktree this service would refuse to read.

**`resolve_owned_session_dir` is the authorisation for a call keyed by session id.** It is `pub` on
`WorktreeServiceImpl`: the token resolves to an OS user, the id (validated as one path segment) to
`<sessions base>/sessions/<id>` under *that user's* sessions base, and the directory must exist. A
session of another user and one that does not exist are the same `NotFound`, so the answer does not
reveal which ids exist. `RestoreSessionWorktree` resolves its session directory through the same
helper (`session_dir_for`), so there is one ownership model. `tddy-daemon-rpc`'s `WatchCodeIndex` calls
it before reading the session's record to follow its worktree's index.

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

## The home worktree (`home_worktree.rs`)

A project's [project-home session](../../../docs/ft/daemon/project-concept.md#project-home-session)
runs on a dedicated checkout, not a per-session branch worktree:

- `home_worktree_path(worktrees_root, project_id)` → `<worktrees_root>/<project_id>/home`.
- `ensure_home_worktree(main_repo, worktrees_root, project_id, main_branch_ref)` returns it when it
  is already one of `main_repo`'s linked worktrees (`git worktree list --porcelain`, compared
  canonicalized); otherwise fetches the ref, checks it resolves to a commit, and runs
  `git worktree add --detach <path> <ref>`. A path there that is not that worktree is
  `HomeWorktreeError::PathTaken`; an unresolvable ref is `UnknownRef`; any failing git command is
  `Git` with git's stderr.
- `fast_forward_home_worktree(home, main_branch_ref)` fetches and `git merge --ff-only`s — the home
  worktree has no commits of its own, so anything but a fast-forward is an error, never a reset.
- A ref is fetched only when its first segment names one of the repository's remotes
  (`origin/main` → `git fetch origin main`); a local branch has nothing to fetch.

## Base freshness (`base_freshness.rs`)

An orchestrator checks its home worktree against its project's trunk by tool, rather than relying on
the fast-forward a start or resume runs. Nothing here runs on a timer.

- `base_freshness(home, trunk_ref)` fetches the trunk ref's branch, then reports a `BaseFreshness`:
  `trunk_ref`, `home_commit`, `remote_tip`, `ahead`, `behind` and `fetched_at_unix_ms`. `behind`
  counts what landed on the remote since the home's commit, not since the last fetch.
- `refresh_home(home, trunk_ref)` fetches and fast-forwards the home to the tip, answering where it
  then stands. A home with commits of its own is `RefreshRefusal::Diverged` (carrying where it
  stands) and is left untouched; a failing git command is `RefreshRefusal::Git`.
- `CutBase { commit, behind }` is the commit a session's worktree was cut from and how many commits
  its upstream has that it does not. `CutBase::of_worktree` reads it; a branch with no upstream has
  no remote tip to be behind and reads zero, while an unreadable `HEAD` or count is an error.
  `CUT_BASE_COMMIT_ARGS` and `CUT_BASE_BEHIND_ARGS` are the git invocations, shared by the local cut
  (run here) and the remote cut (run through the target's shell). `branch_of_worktree` reads the
  branch checked out.

## Session targets (`session_target.rs`)

`resolve_session_target(records, home, target_session_id)` is the one resolver from a session id a
project-home session names to a `SessionTarget` (`session_id`, `project_id`, `worktree`, `branch`,
`base_ref`, `is_active`, `codebase`). It is pure: the daemon builds the `TargetRecord`s from `.session.yaml` and
each session's changeset. The rules, as `SessionTargetError`: an unknown id is `NotFound`, a session
of another project `OtherProject`, and the home session itself `IsHome`. A session with no worktree
— a split session's agent half — still resolves; its `worktree` is `None`, `SessionTarget::worktree()`
refuses it as `NoWorktree` for the tools that need a checkout, and `codebase` names its codebase
half (`CodebaseHalf`: the daemon and workspace session holding the checkout), which the daemon
builds from the record's `codebase_*` fields. The project-home read and git tools resolve every target through it; see
[project-home tools](../../../docs/ft/daemon/project-concept.md#project-home-tools).

## Target sessions' worktrees (`target_worktrees.rs`)

A session whose agent runs on a target records `target` and `repo_path` (the worktree **on the
target**) in its `.session.yaml`. `CleanWorktree`, `RemoveWorktree` and `RestoreSessionWorktree`
route such a session to its target before resolving anything against the host's clone:

- `target_session_in(session_dir)` (Restore) and `target_session_of_worktree(sessions_dir,
  project_id, worktree)` (Clean, Remove) find the `TargetSession` — `None` means a local worktree,
  and the existing path runs unchanged.
- The shell comes through the **`TargetShells`** port, `shell_for(target, subject)`, where `subject`
  is the caller the session token resolves to. The default is `NoTargetShells`
  (`FAILED_PRECONDITION`, "this daemon reaches no session target"); the daemon installs
  `tddy_agent_launch::target_shells::SessionTargetShells` with
  `WorktreeServiceImpl::with_target_shells`. It is a port because the code that builds a target
  shell lives in `tddy-agent-launch`, which depends on this crate.
- `clean_target_worktree` (`git clean -fdx`), `remove_target_worktree` (`git worktree remove
  --force`, then `prune`, in `target_checkout_root_of(worktree)` — `<root>/.worktrees/<id>` →
  `<root>`; `DeleteSession` uses it too) and `restore_target_worktree` (present → unchanged;
  otherwise `prune`, then `worktree add` of the recorded branch, or `fetch --all` and `worktree add
  -b` from the recorded integration base; neither recorded → `FAILED_PRECONDITION`). A non-zero exit
  is `INTERNAL` with its output. Restore never rewrites the changeset.

### Target rows in the stats stream (`target_worktree_stats.rs`)

`StreamWorktreeStats` adds the caller's target sessions of the project
(`target_sessions_of_project(sessions_dir, project_id)`, one metadata read per session) through
`add_target_rows`: each joins the first snapshot as a `CALCULATING` row, and `measure_in_background`
measures it on its target — `git branch --show-current`, `git diff --numstat HEAD`, `du -sk`, each
through `run_on_target` — bounded by `spawn_worker_request_timeout`. The measured row is sent as an
`updated` event; a refused shell, a failing command or the timeout give a `stale` row with no
numbers. `CalculateWorktreeSize` of a target path calls `recalculate_target_row`, which publishes the
new row on the service's `TargetRowBus` — a per-project `broadcast` channel each stream with target
rows forwards (for its own target paths) until the client goes away. Target rows bypass
`WorktreeSizeCalculator`: they are neither semaphore-bounded nor persisted.

`tests/target_worktree_actions_acceptance.rs` drives the three actions and
`tests/target_worktree_stats_acceptance.rs` the stream and `CalculateWorktreeSize` against a
stand-in ssh target (real git on both sides) through the port; both share
`tests/target_session_support/`.

The service's free helpers (`blocking_within`, `worktree_row_from_diff`, the Remove/Clean error
mappings) live in `service_helpers.rs`.

## Session git operations (`session_git_ops.rs`)

The [project-home git tools](../../../docs/ft/daemon/project-concept.md#git-tools) run here, on a
`session_target::SessionTarget`:

- `GitOp` — `Commit { message, paths }`, `Push { force_with_lease }`, `Rebase { onto }`, `Stash`,
  `Reset { mode, to }`, `Checkout { to }`, `Clean`.
- `requires_inactive_target(op)` — false for commit and a plain push, true for everything else.
- `run_git_op(target, op, &PushIdentity)` checks that gate **before** any git process starts
  (`GitOpError::TargetActive { session_id }`), then runs the commands in `session_git_ops/run.rs` in
  the target's worktree with `PushIdentity.env` and `GIT_TERMINAL_PROMPT=0`. A target with no
  checkout of its own is `GitOpError::NoWorktree`. A failing command is
  `GitOpError::Git` with its stderr.
- A rebase that leaves unmerged paths is aborted and answered `GitOpOutcome::RebaseConflict { paths }`;
  a rebase that fails without conflicts is the error.
- Push is `git push [--force-with-lease] origin HEAD:refs/heads/<branch>`.


`tddy-daemon`'s `runtime.rs` builds one `WorktreeServiceImpl` and registers it from the same `Arc`
on all three transports: a `ServiceEntry` from `tddy_service::WorktreeServiceServer::from_arc` for
Connect-HTTP `/rpc` and for the LiveKit common room, and `WorktreeServiceTonicAdapter` on the local
UDS socket, added to the same `Server::builder()` as `ConnectionService`.

**No method here routes to a peer.** A worktree is a directory on the daemon that holds it, and no
request in this service carries a `daemon_instance_id` to route by — which is what makes this
service's wiring simpler than `host.HostService`'s.

`WorktreeRoomCloser` is the seam back to the daemon's session rooms — a worktree that is removed or
restored may need its session's room closed. It is the `SessionRooms` port of
[`tddy-daemon-extensions`](../../tddy-daemon-extensions/docs/extension-ports.md#the-session-rooms-port-session_rooms),
re-exported under this name, and `NoSessionRooms` is the no-op a deployment without rooms uses.

## Worktree sync state (`worktree_sync.rs`)

`read_sync_state(worktree)` reads the current branch (`symbolic-ref`) and its upstream
(`branch.<b>.merge`, then `@{upstream}`). It fetches the branch's recorded remote
(`branch.<b>.remote`) and decides the relation with `merge-base --is-ancestor` in both directions:
`InSync`, `Unpushed`, `Unpulled`, `Diverged`, or `Unpublished` when there is no upstream. It lists the
commits on each side (`log --reverse <a>..<b>`) and the dirty files (`status --porcelain -z
--untracked-files=all`; a rename's second entry is consumed). `SyncState::can_pull` is true only when
unpulled, and `can_push` only when unpushed or unpublished. Git runs with `GIT_TERMINAL_PROMPT=0`, so a
missing credential is an error and never a hung daemon thread. The handlers run it under
`blocking_within` with the request deadline. Nothing here commits.

## The session PR view (`pull_request_view.rs`)

Nothing here calls GitHub. A view is made from four inbox rows: `pr_for_head(branch)`, then
`checks_at(head_sha)`, `conversation(number)` and `pr_commits(number, head_sha)`.
`watch_pull_request` subscribes to `{Pr, Checks, Comments}` for the repository **before** its first
read. It re-reads only on an event that `concerns` the shown view: the branch's `PrForHead` row, the
shown number's `Pr` / `Conversation` / `PrCommits` row, or `ChecksAt` the shown head. It stops when
the client goes away. An inbox failure is sent as a view carrying `unavailable_reason`, and the watch
goes on.

The host answers through the `SessionPullRequests` port: `site_for_session` registers how the
project's repository is read (as the caller) and hands back the inbox and the repo, and
`cached_summary` answers without any request. `DaemonSessionHost`
(`tddy-session-lifecycle`, `svc_trunk_watch_wiring.rs`) implements it over the same inbox and
`HostTrunkSource` the trunk watch uses. The daemon binds it with `attach_pull_requests` once the host
exists, and it can be bound only once. `apply_cached_pull_request` fills `SessionEntry.pr_number` /
`pr_state` during session listing (`tddy-agent-launch`), and listing never waits on GitHub.

## Testing

| Level | Where |
|---|---|
| Unit | each module's own `#[cfg(test)]` — path validation, the stats cache, the branch-ownership rule |
| Integration | `tests/worktrees_rpc.rs`, `worktrees_acceptance.rs`, `worktree_files_rpc.rs`, `stream_worktree_stats_rpc.rs`, `stream_read_worktree_file_acceptance.rs`, `stream_read_worktree_file_rpc_acceptance.rs`, `worktree_session_actions_acceptance.rs`, `worktree_sync_acceptance.rs` (real repos: a bare origin, the session clone, a second clone), `pull_request_view_acceptance.rs` (a real inbox over a scripted GitHub) |
| Component | `packages/tddy-web/cypress/component/WorktreesAppPage.cy.tsx` and the session-inspector suites |

## See also

- [worktrees.md](./worktrees.md) — the library helpers: `git worktree list` parsing, the stats
  snapshots, path validation and removal
- [remote-git-service.md](./remote-git-service.md) — every project as a git remote over tddy-rpc
- Sibling service: [`tddy-host-service`](../../tddy-host-service/docs/host-service.md)
- What stayed: [`tddy-session-lifecycle` session-service.md](../../tddy-session-lifecycle/docs/session-service.md)
- Feature: [docs/ft/web/worktrees.md](../../../docs/ft/web/worktrees.md),
  [docs/ft/web/worktree-disk-usage-streaming.md](../../../docs/ft/web/worktree-disk-usage-streaming.md),
  [docs/ft/web/session-worktree-inspector.md](../../../docs/ft/web/session-worktree-inspector.md),
  [docs/ft/web/session-pr-inspector.md](../../../docs/ft/web/session-pr-inspector.md)
- [changesets/](./changesets/)
