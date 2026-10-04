# 2026-10-04 — Session start phases and code-index warm-up progress

**Type:** Feature

`#live-plan` 12/15 — PR [#571](https://github.com/uppin/tddy-coder/pull/571),
`feature/live-plan/indexing-indicators`. Cross-package entry:
[2026-10-04-indexing-indicators.md](../../../../docs/dev/changesets/2026-10-04-indexing-indicators.md). Product entry:
[2026-10-04-indexing-indicators.md](../../../../docs/ft/web/changelog/2026-10-04-indexing-indicators.md).

`WorktreeServiceImpl::resolve_owned_session_dir(session_token, session_id)` is the authorisation for a
call keyed by session id: token → OS user → `<sessions base>/sessions/<id>` must exist, and a foreign
session and a missing one are the same `NotFound`. `restore_session_worktree` resolves its session
directory through the shared `session_dir_for`, so there is one ownership model. Docs:
[worktree-service.md](../worktree-service.md).

**Tests (scoped).** `another_users_watch_of_the_session_is_refused_and_shows_no_progress`
(`tddy-daemon/tests/code_index_warmup_acceptance.rs`) pins the refusal. `./test -p tddy-worktree-service`
shows `worktree_size_calculator_acceptance::a_cached_size_is_served_after_reload_without_recomputing` and
`remote_git_livekit_acceptance` failing; neither touches this code and they were not compared with a clean
base.

**File length (deferred).** `src/service.rs` 752 → 777 production lines (+25): code-issue record
`oversized-file-service` regressed; the split is deferred under the stack rule (PR #572 touches this file).
