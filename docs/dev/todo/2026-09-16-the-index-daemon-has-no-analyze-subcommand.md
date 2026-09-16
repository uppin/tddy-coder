# 2026-09-16 — `tddy-index-daemon`'s single-shot mode covers restructure but not analyze

**Category:** Future enhancement
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset, M7

The binary has two lifetimes over one service implementation: no transport argument runs one
operation in process and exits, a transport argument serves. `packages/tddy-index-daemon/src/cli.rs`
offers only `restructure` (`apply`, `check`, `anchors`, `status`, `verify`).

The four analysis RPCs — `Coverage`, `Report`, `DuplicateTests`, `Complexity` — are reachable over
gRPC and stdio but not from the binary's own command line. So single-shot mode covers half the
service it hosts, which reads as an oversight rather than a decision.

## What closing it would take

An `analyze` subcommand mirroring `restructure`'s shape: build the request struct, call the trait
method in process, render to stderr, exit with a status. `src/single_shot.rs` and `src/render.rs`
already have the pattern for all five restructure verbs, so this is additive rather than structural.

Note `Coverage` and `DuplicateTests` are the long ones (tens of minutes) and currently cannot be
cancelled — see
[2026-09-16-a-coverage-capture-cannot-be-cancelled.md](./2026-09-16-a-coverage-capture-cannot-be-cancelled.md).
A single-shot `analyze coverage` that `^C` cannot stop is worse on a command line than over an RPC,
so that entry is worth closing first or alongside.
