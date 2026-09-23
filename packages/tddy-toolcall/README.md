# tddy-toolcall

The tool-call protocol between an agent's `tddy-tools` process and the host running its session: the relay listener (`ToolcallRpcService` over `tddy-rpc`/`tddy-stdio`), its client, and the channels submit results and transitions travel through.

## Quick Start

```bash
cargo build -p tddy-toolcall
cargo test -p tddy-toolcall
```

## Dependencies

| Crate | Why |
|---|---|
| `tddy-workflow` | `ClarificationQuestion`, `QuestionOption` |
| `tddy-session-actions` | `ListActions` / `InvokeAction` are answered in the listener |
| `tddy-rpc`, `tddy-stdio` | framing and dispatch |

It never depends on `tddy-core`. `packages/tddy-core/tests/core_facade_shape.rs` pins the
dependency order of every crate carved out of `tddy-core` and holds each at or under 10,000
production lines.

## Module layout

| Module | Owns |
|---|---|
| `toolcall` | `start_toolcall_listener`, `ToolCallRequest`/`ToolCallResponse`, `store_submit_result` / `take_submit_result_for_goal`, `client::dispatch_toolcall`, `client_wire`, `transition`, `build`, `lsp` |

## Relationship to `tddy-core`

This code lived in `tddy-core`, which re-exports this crate whole (`pub use tddy_toolcall::*;`), so
every `tddy_core::…` path it provides still resolves. Write new code against
`tddy_toolcall` directly.

## Documentation

- [Architecture](docs/architecture.md)
- [Changesets](docs/changesets/) — applied changeset history
- [Code issues](docs/code-issues/) — open analyzer and structural findings
