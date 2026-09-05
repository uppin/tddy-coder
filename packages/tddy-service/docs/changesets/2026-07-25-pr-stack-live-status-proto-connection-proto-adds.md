# 2026-07-25 — pr-stack-live-status: `proto/connection.proto` adds `SessionEntry.branch = 28`, `message PrStatusView { exists, number, url, state }`, and two unary RPCs `GetPrStatus(session_id, branch) → PrStatusView` and `RepointPlannedPr(session_id, node_id) → stack_plan_json` (regenerated Rust + `tddy-web/src/gen/connection_pb.ts`). Feature [pr-stack-live-status.md](../../../../docs/ft/coder/pr-stack-live-status.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)

**Type:** Feature


