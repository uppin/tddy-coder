# 2026-10-25 — A project-home session reaches its project's sessions on every host

`ListProjectSessions` now lists the project's sessions on every eligible host, each with the
`daemon_instance_id` of the host it runs on, and names any host that did not answer in
`unreachable_hosts`. `ReadSessionWorktree`, `DiffSessionWorktree` and the seven git tools take an
optional `daemon_instance_id`: a call naming another host is forwarded there and runs under that host's
rules. The receiving host confirms the caller is a project-home session with the daemon that owns it,
so an ordinary session cannot reach another host by addressing a request there. See
[Project-home tools](../project-concept.md#project-home-tools).
