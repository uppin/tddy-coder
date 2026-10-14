# 2026-10-14 — the daemon carries a declared `self` rebind on `VerifyRequest`

**Type:** Feature

`VerifyRequest` gains `repeated string rebinds = 5;` — the `NAME`s the author declares for a
`read_fields_through`, so `verify` can account for the rebound `self`. The daemon's `cli.rs` puts
`--rebind` values on the request, and `queries.rs::serve_verify` hands them to `runner::verify` through
`Options.rebinds`. No new RPC: one field on one existing request, answered by the same comparison.

`tests/dual_transport_acceptance.rs` pins that the declaration crosses the wire and that the CLI and the
daemon render the same lines with and without it; a unit test in `cli.rs` pins that `rebinds` is carried
beside `repoints` and `retargets`.
