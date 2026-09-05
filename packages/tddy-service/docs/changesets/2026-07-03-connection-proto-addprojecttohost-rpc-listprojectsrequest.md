# 2026-07-03 — `connection.proto`: `AddProjectToHost` RPC + `ListProjectsRequest.local_only`

**Type:** Feature

new `AddProjectToHost(AddProjectToHostRequest) returns (AddProjectToHostResponse)` (request: `session_token`, `project_id`, `name`, `git_url`, `main_branch_ref`, `daemon_instance_id`, `user_relative_path`; response: `ProjectEntry project`) for adding an existing project to another host reusing its id; `ListProjectsRequest` gains `bool local_only = 2` to return only local rows and break peer-aggregation recursion. Feature [projects-screen-multi-host.md](../../../../docs/ft/web/projects-screen-multi-host.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service, tddy-daemon, tddy-web)
