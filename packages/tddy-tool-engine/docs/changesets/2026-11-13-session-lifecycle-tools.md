# 2026-11-13 — Session lifecycle tools for orchestrators

**Type:** Feature

`#orchestrators 5/11` (PR [#882](https://github.com/uppin/tddy-coder/pull/882)). Cross-package: [session lifecycle tools](../../../../docs/dev/changesets/2026-11-13-session-lifecycle-tools.md). Product: [Project-home tools](../../../../docs/ft/daemon/project-concept.md#project-home-tools).

- `project_home_lifecycle_tools`: `SetSessionMark`, `DeleteSession`, `ClearWorktree`,
  `DeleteWorktree`, `RestoreWorktree`; `ProjectHomeLifecyclePort` with refusing defaults;
  in `is_project_home_tool`, the catalogue and `is_project_home_target_tool`.
- `src/lib.rs` oversized-file record: 890 → 891. Not split: stack-shared.
