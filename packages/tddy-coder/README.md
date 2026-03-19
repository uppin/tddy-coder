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

CLI binary with goals: `plan`, `acceptance-tests`, `red`, `green`. Backends: `--agent claude` (default), `--agent cursor`, or `--agent claude-acp`. Feature description from stdin or `--prompt`. Writes PRD.md, TODO.md, changeset.yaml to plan directory. TUI (ratatui): scrollable activity log, inbox queue for prompts during Running (Up/Down, E edit, D delete), PageUp/PageDown scroll without mouse capture (text selection works), Ctrl+C restores terminal and cursor. Plan resume: when `--plan-dir` has Init state and no PRD.md, runs plan() to complete. `--agent-output` for raw output. Logging via YAML `log:` section (named loggers, policies with selectors); `--log-level` overrides default policy level. When `--grpc` is set, `StreamTerminal` RPC streams raw ratatui output (ANSI bytes) for remote TUI viewing. Daemon with LiveKit exposes `TerminalService` (per-connection VirtualTui) instead of EchoService; each RPC connection gets its own headless TUI instance.

## Documentation

- [Changesets](./docs/changesets.md) — Applied changeset history