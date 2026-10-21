# 2026-10-21 — the daemon carries a declared detach on `VerifyRequest`

**Type:** Feature

`VerifyRequest` gains `repeated string detaches = 6;` — the `TYPE=NAME` pairs the author declares for a
`detach_method`, so `verify` can account for the detached members and their re-pointed calls. The daemon's
`cli.rs` puts `--detach` values on the request, and `queries.rs::serve_verify` hands them to `runner::verify`
through `Options.detaches`; `tddy-tools`' index client carries the field. No new RPC.

A unit test in `cli.rs` pins that `detaches` is carried beside `rebinds`.
