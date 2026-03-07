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

CLI binary with goals: `plan` (reads feature from stdin, invokes Claude Code, writes PRD.md, TODO.md, .session) and `acceptance-tests` (reads plan from `--plan-dir`, resumes session, creates failing acceptance tests). Supports interactive Q&A (inquire Select/MultiSelect), real-time progress display, `--agent-output` for raw output, and goal-specific exit output.

## Documentation

- [Changesets](./docs/changesets.md) — Applied changeset history