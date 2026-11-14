# 2026-11-14 — Stack merge, repoint and rebase run by the daemon in each node's worktree

The six stack git tools (`pr_merge`, `pr_close`, `pr_repoint`, `pr_set_parents`,
`pr_resolve_conflicts`, `pr_spawn_child`) are daemon exec tools, offered to orchestrator
sessions alone and addressed by `node_id` and `stack_id`. Every git step runs on the daemon in
the node's own worktree; rebases refuse a dirty worktree or a running node session, conflicts
are found without touching the worktree, and pushes go to the project's remote with the
project account's credential.
