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

Core library providing: `CodingBackend` trait for LLM backends, `Workflow` state machine, output parser for delimited PRD/TODO content, and artifact writer. Implements `ClaudeCodeBackend` (production) and `MockBackend` (testing).

## Documentation

- [Architecture](./docs/architecture.md) — Component structure and data flow
- [Changesets](./docs/changesets.md) — Applied changeset history
- [Tech Stack](../../docs/dev/guides/tech-stack.md) — Workspace layout, toolchain
