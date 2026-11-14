# 2026-11-14 — The six stack git tools leave this server for the daemon

**Type:** Feature

The `pr_merge`, `pr_close`, `pr_repoint`, `pr_resolve_conflicts`, `pr_set_parents` and
`pr_spawn_child` methods, their input structs and helpers are removed. The names are forwarded
to the daemon as exec tools like the eleven plan tools (`is_stack_git_tool` dispatch, the
daemon's defs in the advertised catalogue), so a session is offered them only when the daemon
admits it. `server.rs` drops 2,553 → 2,289 production lines; its oversized-file record stays
open and stack-shared, for the post-stack sweep.
