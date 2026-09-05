# 2026-07-25 — branch-query-and-remote-branch: `worktree.rs` adds `push_new_branch_to_origin(worktree_dir, branch) -> Result<(), String>` (`git push -u origin <branch>` via `git_remote_command`, so `GIT_SSH_COMMAND` applies) and a public, non-erroring `worktree_path_for_branch(repo_root, branch) -> Option<PathBuf>` wrapper over `find_existing_worktree_for_branch_ref` (backs the daemon `create_remote_branch` push + `QueryBranch` worktree resolution). Feature [pr-stack-live-status.md](../../../../docs/ft/coder/pr-stack-live-status.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-core)

**Type:** Feature


