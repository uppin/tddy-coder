# 2026-10-15 — Local gh as a project's GitHub source

**Type:** Feature

`ProjectLocalGh.tsx` (the Local gh toggle, `HostGithubState`, `githubHostnameOf`) and `ProjectAccountAssignment.tsx`, split out of `ProjectsScreen.tsx`; `ProjectsScreen` takes `loadHostGithubCli` and sends `localGh`; `ProjectsAppPage` wires `HostService.getHostTooling`. See [the documentation](../projects-screen.md#local-gh).
