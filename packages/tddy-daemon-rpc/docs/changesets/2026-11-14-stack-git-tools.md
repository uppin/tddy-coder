# 2026-11-14 — The six stack git tools are served by this daemon in each node's worktree

**Type:** Feature

`exec_tool/stack_git_tools.rs` (999 production lines) serves `tddy-tool-engine`'s six git
tools beside the plan tools: `ExecToolRpcHandler::run_stack_git_tool` classifies the caller
(`stack_caller_of`), serves only an orchestrator, and forwards a call naming another host one
hop. `DaemonStackGitTools` (the plan tools' port whole, plus the project-home port) runs every
git step in the node's own worktree: `pr_repoint` / `pr_set_parents` realign through
`tddy-pr-stack`'s `rebase_onto`, refuse a dirty worktree or a running node session untouched,
and force-push with lease to the project's remote; `pr_resolve_conflicts` probes with
`git merge-tree --write-tree` and, with `delegate`, sends or resumes the node's session with
the conflicting paths; `pr_spawn_child` starts the node through the start path with the caller
as stack parent and binds it; `pr_merge` / `pr_close` reach GitHub as the project account,
with the merge's `StackOpJournal` beside the stack. The file's split is deferred to the
stack's sweep node and recorded in `docs/code-issues/oversized-file-stack-git-tools.md`.
