# 2026-11-14 — The `#orchestrators` sweep: stack-git split, the session stack guard, and `CreateProjectStack`

**Type:** Refactor

`#orchestrators` 12/12, PR [#901](https://github.com/uppin/tddy-coder/pull/901). Feature:
[PR stacking § Opening a project stack by id](../../../docs/ft/coder/pr-stacking.md#opening-a-project-stack-by-id).

## What changed

- **B1** — `exec_tool/stack_git_tools.rs` split: the git work moved to `stack_git_tools::git_work`
  (429 lines) and `git_work::node_ops` (217 lines); the parent file is 432 lines.
- **B3** — the `add_project_to_host` handler moved from `project/coordinate_handlers.rs` to
  `project::add_to_host`; `coordinate_handlers.rs` is 444 lines.
- **B9** — the `session_id`-addressed PR-stack mutations refuse a session naming no stack,
  whatever its recipe (`require_a_stack`, `FailedPrecondition`, naming `pr_stack_create`);
  the recipe guard in `pr_stack/guards.rs` is deleted. `CreateProjectStack` creates a stack by
  address through `ProjectStackStore::create_titled`, forwarded to the owning host.
- **T1** — the spawner scope check matches on `read_changeset` and refuses with a message naming
  the unreadable changeset.

## Code issues

- `oversized-file-stack-git-tools.md` deleted: 999 → 432 production lines (final measurement above).
- `oversized-file-project-coordinate-handlers.md` deleted: 566 → 444 production lines.
- `oversized-file-pr-stack-ports.md` kept open: 868 → 876 production lines. Accepted as standing
  debt by developer consent, 2026-11-14, reason "restructure engine refused" — see
  `docs/dev/todo/2026-11-14-restructure-engine-path-module-blocks-moves.md` and
  `docs/dev/todo/2026-11-14-restructure-engine-no-extract-for-guarded-ranges.md`.

## Resolved backlog

- `docs/dev/todo/2026-11-13-spawner-scope-check-reads-changeset-with-ok.md` (T1) — deleted in this PR.
