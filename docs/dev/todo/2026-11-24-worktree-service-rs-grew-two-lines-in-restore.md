# 2026-11-24 — `tddy-worktree-service/src/service.rs` grew 2 lines past its oversized baseline

**Category:** Deferred decomposition (developer consent, 2026-11-24)
**Source:** `/pr-wrap` step 3.5 (file-length gate) on `feature/session-close-and-remove` (PR #1032)

`service.rs` was already over the 500-production-line budget at 679 lines. The session-close change
adds two lines inside `impl WorktreeService for WorktreeServiceImpl::restore_session_worktree`:
`forget_a_recorded_removal(&session_dir)?;` on the target path and on the local path. A restored
worktree has to clear the `worktree-removed` marker a close wrote. That makes the file 681 lines.

The restructure engine cannot cut inside a trait impl
(`docs/dev/todo/2026-11-14-restructure-engine-no-extract-for-guarded-ranges.md`), and the
developer chose to defer instead of hand-folding the restore tail into a helper.

## What would close it

Either of these, then re-measure `packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md`:

- The engine extracts from trait impls, and the record's seams B and C move.
- A consented hand fold: both success tails of `restore_session_worktree` (clear the marker,
  invalidate the stats cache, build the response) become one helper in `service_helpers.rs`. That
  takes the file back below 679.
