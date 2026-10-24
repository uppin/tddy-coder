# 2026-10-24 — CreateProject and AddProjectToHost clone without a deadline

**Type:** Feature

`#start-progress` 5/8 ([#738](https://github.com/uppin/tddy-coder/pull/738)). Both RPCs clone through
`ProjectRpcHandler::clone_without_deadline` (supervisor, forked worker or in-process), with no
`spawn_blocking_with_timeout` / `await_supervised_with_timeout` wrapper. They stay unary and report no
progress (`docs/dev/todo/2026-10-24-stream-create-project-and-add-to-host-clone-progress.md`).

Code issues: `add_project_to_host_at_project_coordinate` 153 → 119 lines, `create_project_at_project_coordinate`
109 → 75, `project/coordinate_handlers.rs` 586 → 564 production lines — better, not clean; records narrowed.
