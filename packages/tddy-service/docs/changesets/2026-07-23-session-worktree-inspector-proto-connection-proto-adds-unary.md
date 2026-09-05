# 2026-07-23 — session-worktree-inspector: `proto/connection.proto` adds unary `CleanWorktree` + `RestoreSessionWorktree` RPCs and their request/response messages (`CleanWorktreeRequest{session_token,project_id,worktree_path}`/`Response{ok,message}`, `RestoreSessionWorktreeRequest{session_token,project_id,session_id}`/`Response{ok,message,worktree_path}`). Regenerated Rust + `tddy-web/src/gen/connection_pb.ts`. Feature [session-worktree-inspector.md](../../../../docs/ft/web/session-worktree-inspector.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)

**Type:** Feature


