# 2026-07-27 — **remove-origin-hardcode

**Type:** Bug Fix

`connection.proto` exposes the resolved default remote to clients** — `ProjectEntry` gains `string default_remote = 7` (resolved default remote for the project's main repo; empty when undetected) and `ListProjectBranchesResponse` gains `string default_remote = 2`, both populated by the daemon from `effective_remote_name_for_project` so clients can normalize `<remote>/<branch>` picker values to local branch names without assuming `origin`. The `main_branch_ref` doc comment is generalized from `origin/<branch>` to `<remote>/<branch>`. Regenerated Rust + `packages/tddy-web/src/gen/connection_pb.ts`. Feature [git-integration-base-ref.md](../../../../docs/ft/coder/git-integration-base-ref.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)
