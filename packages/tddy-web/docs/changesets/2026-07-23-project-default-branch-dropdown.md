# 2026-07-23 — Project default-branch dropdown

**Type:** Feature

each `ProjectCard` gains a default-branch `<select>` (`project-default-branch-select-<projectId>`) listing the project's remote branches (loaded via `listProjectBranches` for the project's first host); the stored `ProjectEntry.mainBranchRef` shows selected, else it pre-selects `origin/master` (then `origin/main`, then the first branch) via `defaultSelectedBranch`. Choosing a branch calls the new `setProjectDefaultBranch` RPC (wired in `ProjectsAppPage`'s `useProjectsRpc`, addressed to the first host) and refreshes the list. `ProjectsScreen`/`ProjectCard` gain `onSetDefaultBranch` + `loadProjectBranches` props; `ProjectGroup` carries `mainBranchRef`. Regenerated `connection_pb.ts`. Tests: `ProjectsScreenAcceptance` 12 (7 existing + 5 new). Feature [projects-screen-multi-host.md](../../../../docs/ft/web/projects-screen-multi-host.md#default-branch). (tddy-web)
