# 2026-10-24 — Clone step and clone progress on the start stream

**Type:** Feature

`#start-progress` 5/8 ([#738](https://github.com/uppin/tddy-coder/pull/738)). `session.proto`:
`StartPhase.Step.STEP_CLONE = 4`; `CloneProgress { stage, percent, objects_done, objects_total,
bytes_received, bytes_per_second, line }`; `StartSessionEvent.clone_progress = 5`. See
[start-session-phases.md](../start-session-phases.md). Cross-package entry:
[docs/dev/changesets/2026-10-24-start-progress-clone-progress.md](../../../../docs/dev/changesets/2026-10-24-start-progress-clone-progress.md).
