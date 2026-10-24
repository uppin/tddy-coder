# 2026-10-24 — Set up "main" without leaving the Sessions screen

- Clicking a project's pinned **main** entry when the project has no home agent opens a setup pane
  at `#/sessions/project-home/<id>`: choose the home agent, then **Start main session**. The hint
  under the entry is gone. See [session drawer § Pinned main entry](../session-drawer.md#pinned-main-entry)
  and [URL state routing](../url-state-routing.md).
- `#/projects?project=<id>` still focuses that project's home-agent picker, as a deep link. See
  [Projects § Home agent](../projects-screen-multi-host.md#home-agent).
