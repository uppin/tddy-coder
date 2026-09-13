# 2026-09-12 — Catalogue, exec-tool and PR-stack coordinates on the wire

`connection.ConnectionService` reaches **17** methods. This package registers
`catalog.CatalogService`, `exec_tools.ExecToolService` and `pr_stack.PrStackService` on every
transport, including session-room LiveKit rosters, and implements family P in `pr_stack_rpc.rs` with
peer routing in `svc_pr_stack_ports.rs`.

Cross-package changeset:
[docs/dev/changesets/2026-09-12-unbundle-exec-prstack-services.md](../../../../docs/dev/changesets/2026-09-12-unbundle-exec-prstack-services.md).

Docs: [connection-service.md](../connection-service.md), [pr-stack-service.md](../pr-stack-service.md).
