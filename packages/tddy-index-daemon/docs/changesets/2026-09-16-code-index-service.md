# 2026-09-16 — code_index.CodeIndexService

**Type:** Feature

New crate. Holds a warm rust-analyzer index per workspace root and serves eleven operations — the
plan-driven restructuring five plus coverage, CRAP reporting, duplicate-test detection and
per-function complexity — as `code_index.CodeIndexService`.

One binary, two lifetimes over one implementation instance: no transport argument runs a single
operation against the generated service trait in process, passing prost structs with no encode or
decode, and exits with a status; `--grpc`, `--grpc-uds` and/or `--stdio` serve and stay alive,
concurrently and sharing one index. Neither a subcommand nor a transport is an error rather than a
default.

Every request names its `workspace_root`. Requests touching the tree or the journal are serialized
per root, because `.restructure/` is keyed by root with no lock file; `Warm`, `Workspaces` and
`Complexity` are not, because they touch neither.

Long operations stream, which is what gives a handler a back-channel: a send failing into a dropped
receiver is the only signal `tddy_rpc` gives that a caller has gone away, and it cancels the work.
Refusals go through one exhaustive `match` per error type with no catch-all arm, so a variant added
later is a compile error rather than a silent `Internal`.

Per request the daemon logs the method, the root, **whether that root was already warm**, the outcome
and the duration. `Workspaces` logs at `DEBUG` — it names no root and is what a dashboard polls.

See [code-index-service.md](../code-index-service.md).
