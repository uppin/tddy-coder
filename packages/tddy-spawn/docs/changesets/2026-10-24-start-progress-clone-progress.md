# 2026-10-24 — A clone reports git's progress, lands whole and can be stopped

**Type:** Feature

`#start-progress` 5/8 ([#738](https://github.com/uppin/tddy-coder/pull/738)). New `git_progress`
(`parse`, `GitProgress`, `ProgressThrottle`, `PROGRESS_INTERVAL`) and `clone_run`
(`clone_with_progress`, `CloneHandle`): `git clone --progress` into `<dest>.partial-<uuid>`, stderr read
as written, rename on success, removal on failure, the clone in its own process group.
`clone_as_user_observed` is deleted; `clone_as_user_with_env` is a thin call into `clone_with_progress`.
The worker sends `WorkerEvent::CloneProgress`. See [clone-run.md](../clone-run.md) and
[spawn-worker.md](../spawn-worker.md).

Code issues: this crate has no `docs/code-issues/`; `spawner.rs` only lost code (see
`docs/dev/todo/2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md`). The new modules
are 267 and 131 lines; no function over 60.
