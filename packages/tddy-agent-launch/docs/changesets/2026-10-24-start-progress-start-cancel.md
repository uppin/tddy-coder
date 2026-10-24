# 2026-10-24 — A streamed start runs as a cancellable task

**Type:** Feature

`#start-progress` 8/8 (#741). `start_task.rs` registers a streamed start as a `start_session` task that names itself first. Its token stops the clone (`clone_stopper.rs`, `CloneBackend::clone_repo`'s `on_started`), is checked after the clone, and a start answered after a cancel has its session deleted before the stream ends `CANCELLED`. Layout: [module-layout § A streamed start is a task](../module-layout.md#a-streamed-start-is-a-task). `start_session_on_request` 381 → 391 lines (code issue kept, deferred with the developer's consent). Cross-package entry: [changeset](../../../../docs/dev/changesets/2026-10-24-start-progress-start-cancel.md).
