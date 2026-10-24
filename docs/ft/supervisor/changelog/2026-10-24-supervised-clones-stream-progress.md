# 2026-10-24 — Supervised clones stream git's progress and can be stopped

On a host run by `tddy-supervisor`, a clone now reports git's progress like any other host, a failed
clone's error carries git's own message instead of only its exit code, and a clone can be stopped. The
new `CloneRepository` operation runs `git clone --progress` as the session user under the same policy as
`SpawnSession`. Operators: git is found on the **supervisor's** `PATH`, and that path must be in
`spawn_policy.allowed_tool_paths`. See [tddy-supervisor](../tddy-supervisor.md#what-it-does).
