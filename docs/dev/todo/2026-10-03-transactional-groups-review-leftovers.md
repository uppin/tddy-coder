# Leftovers from the transactional-groups review

**Date:** 2026-10-03
**Found by:** the `#live-plan 10/15` (transactional groups) review passes. Each item was judged out of that PR's scope or a trade-off, not an oversight.

## Defect

- `Journal::group_records` / `group_pre_images` read from the **last** `group_started` to the **end of the journal**, not to that group's own end. Harmless while a rollback is always of the latest group; wrong the moment anything else is journalled after an open group.

## Clean-code debt

- The CLI loop (`runner/entry_points/store_run.rs`) and the daemon loop (`tddy-index-daemon/src/apply.rs`) duplicate the group sequence (gate construction, enter, commit, settle, refresh). They have already diverged once. One shared driver would remove it.
- `apply_held_plan` is ~145 lines in each loop; `record_applied_op`, `GroupRun::enter`, `settle` and `refresh_plan` take 6–7 parameters.
- `render.rs::applied_lines` and `tddy-tools/src/index_console.rs::operation_lines` are the same body, with duplicated tests; one belongs in `console::`.
- `OpenGroup.members` / `.pre_images` are populated and never read; several `GroupRun` methods are `pub` but used only through `enter` / `settle`.

## Limitations

- `PreImage::capture` uses `read_to_string`: a non-UTF-8 file in a group's edit set fails the whole apply (before anything is committed).
- Rollback removes created files but not the directories a member created; "byte for byte" is a statement about files.
- `--from` or `stop_after` landing **inside** a group applies a truncated group (only the start-of-group `stop_after` case is handled).
- The daemon passes a no-op `on_check`, so a multi-minute group check is silent there.
- A cancel during resolve or commit leaves the group open for the resume to roll back, while the daemon's between-members cancel rolls back at once.
- The daemon's group handling has no test of its own; the acceptance suite drives the CLI path.
- The stale-member refusal waits on the live-plans work.
