# tddy-coder

TDD-driven coder CLI for PRD-based development workflow.

## Quick Start

### Development
```bash
cargo build -p tddy-coder
```

### Testing
```bash
cargo test -p tddy-coder
```

### Run
```bash
echo "Build a user authentication system" | cargo run -p tddy-coder -- --goal plan --output-dir ./plans
```

## Architecture

CLI binary that reads feature descriptions from stdin, invokes Claude Code in plan mode via tddy-core, and writes PRD.md and TODO.md to a named output directory.

## Documentation

- [Changesets](./docs/changesets.md) — Applied changeset history