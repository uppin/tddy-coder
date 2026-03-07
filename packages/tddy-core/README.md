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

Core library providing: `CodingBackend` trait for LLM backends, `Workflow` state machine, NDJSON stream parser for Claude Code CLI, output parser for PRD/TODO and acceptance-tests (structured-response and delimited), artifact writer, and changeset.yaml persistence. Implements `ClaudeCodeBackend` (production) and `MockBackend` (testing). Supports plan, acceptance-tests, red, and green workflow steps. Changeset stores initial_prompt, clarification_qa, sessions (with system_prompt_file per session), discovery, and workflow state.

## Documentation

- [Architecture](./docs/architecture.md) — Component structure and data flow
- [Changesets](./docs/changesets.md) — Applied changeset history
- [Tech Stack](../../docs/dev/guides/tech-stack.md) — Workspace layout, toolchain
