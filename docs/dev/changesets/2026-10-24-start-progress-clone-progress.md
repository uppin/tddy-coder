# 2026-10-24 — A start that clones reports git's progress; clones are unbounded and land whole

**Type:** Feature · PR [#738](https://github.com/uppin/tddy-coder/pull/738) (`#start-progress` 5/8)

A session start that has to clone its project reports a `STEP_CLONE` step and git's parsed progress
(`CloneProgress`: stage, percent, objects, bytes, throughput), throttled where git runs. Clones run
`git clone --progress` into `<dest>.partial-<uuid>` and are renamed into place only on success; a failed
or stopped clone removes the temporary directory, and `CloneHandle::stop` ends git's whole process group.
The session-start, `CreateProject` and `AddProjectToHost` clones have no deadline. The supervised backend
brackets the step with no progress (`#start-progress` 6/8 streams it). Product docs:
[Project concept § Cloning at session start](../../ft/daemon/project-concept.md#cloning-at-session-start),
[`StartPhase`](../../../packages/tddy-service/docs/start-session-phases.md),
[running a clone](../../../packages/tddy-spawn/docs/clone-run.md).

Decisions:

- **The `tddy-projects` cloner signature is kept** (`Fn(&str, &Path) -> Result<(), String>`): the
  closure built in `spawn_project_clone` captures the start's `AttachmentProgressSink`
  (`clone_as_a_start_step`), so `tddy-projects` is untouched.
- **Temporary sibling + rename** over remove-on-failure alone: same filesystem, atomic, and a daemon
  that dies mid-clone leaves only a `.partial-*` directory no "already present" check matches.
- **Throttle at the source** (≤ 4/s plus every stage change and 100%), so neither the worker pipe nor
  the LiveKit relay carries git's per-tick redraws.
- **Complexity 5 accepted**: the proto event, the parser and the emitter are one capability.

Resolved backlog entry: *A timed-out clone leaves a partial destination that later starts skip*
(`2026-10-17-a-timed-out-clone-leaves-a-partial-destination-that-later-starts-skip`) — deleted.

Deferred: [`2026-10-24-clone-no-deadline-tests-need-a-slow-clone-seam`](../todo/2026-10-24-clone-no-deadline-tests-need-a-slow-clone-seam.md)
(the two "outlives the old timeout" acceptance tests: no production seam stages a slow clone) and
[`2026-10-24-stream-create-project-and-add-to-host-clone-progress`](../todo/2026-10-24-stream-create-project-and-add-to-host-clone-progress.md).

Code issues (structural re-measurement, brace matching; coverage/CRAP not re-derived):
`ensure_project_available_for_start` 108 → 103, `add_project_to_host_at_project_coordinate` 153 → 119,
`project/coordinate_handlers.rs` 586 → 564 production lines — all better, not clean, records narrowed.
No new function over 60 lines, no new file over 500.

- `tddy-service` — `STEP_CLONE`, `CloneProgress`, `StartSessionEvent.clone_progress`:
  [entry](../../../packages/tddy-service/docs/changesets/2026-10-24-start-progress-clone-progress.md)
- `tddy-spawn` — `git_progress`, `clone_run`, `WorkerEvent::CloneProgress`:
  [entry](../../../packages/tddy-spawn/docs/changesets/2026-10-24-start-progress-clone-progress.md)
- `tddy-session-files` — `AttachmentProgressSink::clone_progress`:
  [entry](../../../packages/tddy-session-files/docs/changesets/2026-10-24-start-progress-clone-progress.md)
- `tddy-agent-launch` — the clone step, no deadline:
  [entry](../../../packages/tddy-agent-launch/docs/changesets/2026-10-24-start-progress-clone-progress.md)
- `tddy-daemon-rpc` — `clone_without_deadline`:
  [entry](../../../packages/tddy-daemon-rpc/docs/changesets/2026-10-24-start-progress-clone-progress.md)
- `tddy-web`, `tddy-rust-typescript-tests` — `session_pb.ts` regenerated only.
