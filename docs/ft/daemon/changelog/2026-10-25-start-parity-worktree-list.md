# 2026-10-25 — A project-home session sees the project's worktrees as the Worktrees screen does

Two new project-home tools. `ListWorktrees` lists the project's worktrees with the Worktrees screen's
fields and in its order (largest first, never-sized last, then by name), a page at a time (50 by
default, at most 200), with how many are still being sized. `GetWorktree` returns one of them with
the session that owns it. Both take `daemon_instance_id` to read another host. They read the
screen's own snapshot and never start a size calculation. See
[Project-home tools](../project-concept.md#project-home-tools).
