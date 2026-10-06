# 2026-10-07 — the `Snapshot` RPC answers a headerless plan without a server

**Type:** Tests

No source change: the registered `Snapshot` coordinate already routes a plan with no item anchors to
the in-process `snapshot`. One test in `tests/code_index_service_acceptance.rs` pins it — a headerless
plan over a symbol anchor is snapshotted through the RPC, `(paths, rewritten, stale) == (1, true, [])`,
and `Workspaces` lists no held root.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-07-plan-header.md).
