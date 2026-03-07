# tddy-core

Core library for tddy-coder.

## Quick Start

### Development
```bash
cargo build -p tddy-core
```

### Testing
```bash
cargo test -p tddy-core
```

## Architecture

Core library providing: `CodingBackend` trait for LLM backends, `Workflow` state machine, NDJSON stream parser for Claude Code CLI, output parser for PRD/TODO and acceptance-tests (structured-response and delimited), artifact writer, and session file persistence. Implements `ClaudeCodeBackend` (production) and `MockBackend` (testing). Supports `plan` and `acceptance_tests` workflow steps.

## Documentation

- [Architecture](./docs/architecture.md) — Component structure and data flow
- [Changesets](./docs/changesets.md) — Applied changeset history
- [Tech Stack](../../docs/dev/guides/tech-stack.md) — Workspace layout, toolchain
