# tddy-log

The tddy log sink: config-driven loggers and policies, multi-output routing, startup rotation, and the guards that keep log output off a stdio a TUI or an RPC relay owns.

## Quick Start

```bash
cargo build -p tddy-log
cargo test -p tddy-log
```

## Dependencies

No workspace dependencies — a leaf.

It never depends on `tddy-core`. `packages/tddy-core/tests/core_facade_shape.rs` pins the
dependency order of every crate carved out of `tddy-core` and holds each at or under 10,000
production lines.

## Module layout

| Module | Owns |
|---|---|
| `log_backend` | `LogConfig` (loggers, policies, selectors), `TddyLogger`, `init_tddy_logger`, rotation, the TUI log buffer |
| `stdio_safety` | `enforce_stdio_safe_log_output`, `redirect_fd_to_file` for `--stdio` processes |

## Relationship to `tddy-core`

This code lived in `tddy-core`, which re-exports this crate whole (`pub use tddy_log::*;`), so
every `tddy_core::…` path it provides still resolves. Write new code against
`tddy_log` directly.

## Documentation

- [Architecture](docs/architecture.md)
- [Changesets](docs/changesets/) — applied changeset history
- [Code issues](docs/code-issues/) — open analyzer and structural findings
