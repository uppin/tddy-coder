# 2026-07-25 — branch-query-and-remote-branch: `proto/connection.proto` adds a unary `QueryBranch(QueryBranchRequest{session_token,session_id,branch}) → QueryBranchResponse{BranchResolution{branch, BranchSession{exists,session_id,is_active,status}, BranchWorktree{exists,path}, PrStatusView pr}}` RPC (reuses `PrStatusView`; additive alongside `GetPrStatus`) plus `StartSessionRequest.create_remote_branch = 28` (regenerated Rust + `tddy-web/src/gen/connection_pb.ts`). Feature [pr-stack-live-status.md](../../../../docs/ft/coder/pr-stack-live-status.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)

**Type:** Feature


