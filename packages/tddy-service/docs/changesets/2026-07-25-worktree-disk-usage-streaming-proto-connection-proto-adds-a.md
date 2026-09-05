# 2026-07-25 — worktree-disk-usage-streaming: `proto/connection.proto` adds a `WorktreeSizeStatus` enum + `WorktreeRow.{size_status, size_calculated_at_unix_ms}`, a server-streaming `StreamWorktreeStats(StreamWorktreeStatsRequest{recalculate_all}) → stream WorktreeStatsEvent{snapshot, updated}`, and unary `CalculateWorktreeSize` (regen Rust + `tddy-web/src/gen/connection_pb.ts`). Feature [worktree-disk-usage-streaming.md](../../../../docs/ft/web/worktree-disk-usage-streaming.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)

**Type:** Feature


