# 2026-10-24 — A start's clone is a step with progress, and has no deadline

**Type:** Feature

`#start-progress` 5/8 ([#738](https://github.com/uppin/tddy-coder/pull/738)). `clone_as_a_start_step`
brackets a real clone with `STEP_CLONE` and feeds git's progress into the sink
(`clone_backend::clone_progress_event`); `CloneBackend::clone_repo` takes `on_progress`; the
`tokio::time::timeout(spawn_worker_request_timeout, …)` around provisioning is removed. See
[module layout § Cloning a project](../module-layout.md#cloning-a-project-for-a-start). Cross-package
entry: [docs/dev/changesets/2026-10-24-start-progress-clone-progress.md](../../../../docs/dev/changesets/2026-10-24-start-progress-clone-progress.md).

Code issues: `ensure_project_available_for_start` 108 → 103 lines (better, not clean; record narrowed).
