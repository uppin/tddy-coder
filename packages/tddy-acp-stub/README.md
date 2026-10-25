# tddy-acp-stub

ACP agent stub for testing ClaudeAcpBackend without real Claude.

## Quick Start

### Build
```bash
cargo build -p tddy-acp-stub
```

### Run (stdio ACP server)
```bash
cargo run -p tddy-acp-stub -- --scenario /path/to/scenario.json
```

## Architecture

Implements `acp::Agent` from agent-client-protocol. Reads JSON scenario from `--scenario <path>` or `TDDY_ACP_SCENARIO` env var. Scenario defines responses (chunks, tool_calls, permission_requests, stop_reason). A scenario's `models` list is advertised on `session/new` and `session/load`; a response with `hold_until_cancel` keeps its turn open until `session/cancel` arrives and then ends `cancelled`. With `TDDY_ACP_STUB_RECORD=<path>` the stub appends one JSON line per event it sees (`started` with its pid, `new_session` with cwd and MCP server names, `load_session`, `set_session_model`, `cancel`, `permission_outcome`), so tests observe what the client asked. Used by tddy-agent-backend's ACP driver and backend acceptance tests.

## Documentation

- [tddy-agent-backend docs](../tddy-agent-backend/docs/architecture.md) — ClaudeAcpBackend architecture
