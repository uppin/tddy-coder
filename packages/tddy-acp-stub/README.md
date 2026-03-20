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

Implements `acp::Agent` from agent-client-protocol. Reads JSON scenario from `--scenario <path>` or `TDDY_ACP_SCENARIO` env var. Scenario defines responses (chunks, tool_calls, permission_requests, stop_reason). Used by tddy-core ACP acceptance tests.

## Documentation

- [tddy-core docs](../tddy-core/docs/architecture.md) — ClaudeAcpBackend architecture
