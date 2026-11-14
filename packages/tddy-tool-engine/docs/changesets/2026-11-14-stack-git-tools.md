# 2026-11-14 — The six stack git tools, offered to orchestrators alone

**Type:** Feature

`stack_git_tools` is the git sibling of n7's `stack_tools` family: `pr_merge`, `pr_close`,
`pr_repoint`, `pr_set_parents`, `pr_resolve_conflicts` and `pr_spawn_child` with their schemas
(each names its `node_id`, and accepts `stack_id` and `daemon_instance_id`), `StackGitOp`,
`StackGitToolPort`, and the offering — an orchestrator-role caller is offered all six, a session
bound to a node of a stack and every other session none. The six join the project-home catalogue
and the read-only allowed set. `lib.rs` grew one module declaration (892 → 893 production
lines); its oversized-file record is stack-shared and stays open for the post-stack sweep.
