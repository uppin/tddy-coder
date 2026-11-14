# 2026-11-13 — The daemon serves the session lifecycle tools

**Type:** Feature

`#orchestrators 5/11` (PR [#882](https://github.com/uppin/tddy-coder/pull/882)). Cross-package: [session lifecycle tools](../../../../docs/dev/changesets/2026-11-13-session-lifecycle-tools.md). Product: [Project-home tools](../../../../docs/ft/daemon/project-concept.md#project-home-tools).

- `exec_tool/project_home/lifecycle.rs`: `ProjectHomeLifecyclePort` for `DaemonProjectHome`
  over the Worktrees screen's functions; all five tools forwarded to another host.
