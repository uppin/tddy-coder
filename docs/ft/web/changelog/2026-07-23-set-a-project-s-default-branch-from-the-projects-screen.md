# 2026-07-23 — Set a project's default branch from the Projects screen

- Each project card gains a **default branch** dropdown listing the project's remote branches (sourced from `ListProjectBranches`); choosing one sets the project's `main_branch_ref` via the new `SetProjectDefaultBranch` RPC and applies it across the project's hosts ([projects-screen-multi-host.md](../projects-screen-multi-host.md#default-branch)).
- A project with no stored default pre-selects `origin/master` when present, otherwise `origin/main` — matching the live default-resolution order — so a sensible default is always shown without implying one has been persisted. Any remote branch (including slash-containing names) is selectable.
