# 2026-10-07 — the daemon carries a declared `impl` retarget on `VerifyRequest`

**Type:** Feature

`VerifyRequest` gains `repeated string retargets = 3;` — the `OLD=NEW` retargets the author declares,
so `verify` can account for them. The daemon's `cli.rs` puts `--retarget` values on the request, and
`queries.rs::serve_verify` hands them to `runner::verify` through `Options.retargets`. No new RPC and
no new service: one field on one existing request, answered by the same comparison.

`tests/dual_transport_acceptance.rs` pins that the declaration crosses the wire and that the CLI cold
path and the daemon render the same lines, with and without the flag.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-07-retarget-impl.md).
