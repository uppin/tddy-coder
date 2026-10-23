# 2026-10-23 — "main" with no home agent takes you to set one

- **Fixed:** clicking a project's pinned **main** entry did nothing when the project had no home
  agent. It opens **Projects** with that project's **Home agent** picker focused instead; Back
  returns to the drawer. See [session drawer § Pinned main entry](../session-drawer.md#pinned-main-entry).
- **`#/projects?project=<id>`** focuses that project's home-agent picker. See
  [Projects § Home agent](../projects-screen-multi-host.md#home-agent).
