# 2026-09-06 — No code path issues `GetHostTooling` yet

**Category:** Host tooling probe
**Source:** `#hosts-screen` 4/8, PR #456

- The RPC, `host_tooling.rs` and `HostRowTooling` each carry their own tests, but `HostRowTooling`
  is **not mounted**: node 1 of the stack owns the Hosts row, and mounting the section from node 4
  would have edited a file that PR owns. Every piece is unit-covered; the feature is **not proven
  end to end**.
- Consequence to keep in view: the two blockers this node found in green (a `PATH` resolution that
  could never have exec'd anything, and a `gh` `NotFound` mapped to "not installed") were both
  invisible to a green test run. Nothing exercises the assembled path until the row mounts the
  section.

## Narrowed 2026-10-15 (local gh)

The Projects card is now the first production caller of `GetHostTooling`: for each host owning a Local gh
project it asks `GetHostTooling { daemonInstanceId, githubHostname }` and shows that host's `gh` state
(`ProjectLocalGh.cy.tsx`, `ProjectsAppPage` wiring). **What stays open:** the Hosts screen still does not mount
`HostRowTooling`, so the Hosts-row path is not proven end to end.

