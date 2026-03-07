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

CLI binary with goals: `plan`, `acceptance-tests`, `red`, `green`. Backends: `--agent claude` (default) or `--agent cursor`. Feature description from stdin or `--prompt`. Writes PRD.md, TODO.md, changeset.yaml to plan directory. Supports interactive Q&A (inquire Select/MultiSelect), real-time progress display, `--agent-output` for raw output.

## Documentation

- [Changesets](./docs/changesets.md) — Applied changeset history