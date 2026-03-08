# Technology Stack

This document outlines the technology stack used in this project for consistent technology decisions.

## Core Technologies

1. **Rust** — Primary programming language for systems programming, performance, and memory safety
2. **Cargo** — Package manager and build system
3. **Edition 2021** — Stable Rust edition (or latest stable when available)

## Development Tools & Build System

1. **rustup** — Rust toolchain installer and version manager
2. **cargo** — Build, test, and dependency management
3. **cargo-watch** — Re-run commands on file changes (optional, for rapid feedback)
4. **rustfmt** — Code formatting
5. **clippy** — Linting and idiomatic Rust suggestions

## Testing & Quality Assurance

1. **cargo test** — Built-in test runner for unit and integration tests
2. **criterion** — Benchmarking for performance-critical code
3. **proptest** or **quickcheck** — Property-based testing
4. **mockall** — Mocking for unit tests

## Code Organization

### Monorepo Workspace

Root `Cargo.toml` defines the workspace. Rust crates live in `packages/`:

```
tddy-coder/
├── Cargo.toml              # Workspace manifest
├── Cargo.lock              # Shared lockfile
├── flake.nix               # Nix development shell (rustc, cargo, clippy, rust-analyzer)
├── packages/
│   ├── tddy-core/          # Rust library crate
│   └── tddy-coder/         # CLI binary crate
```

- **Build from root**: `cargo build` (all crates) or `cargo build -p tddy-core` (single crate)
- **Test from root**: `cargo test` or `cargo test -p tddy-core`
- **Add new crate**: Create `packages/{name}/` with `Cargo.toml` and add to workspace `members` in root `Cargo.toml`

### Crate Layout

- **Binary crates**: `src/main.rs` for executables
- **Library crates**: `src/lib.rs` for reusable code
- **Integration tests**: `tests/` directory at crate root
- **Benchmarks**: `benches/` directory with criterion

### Crate Structure

```
my-crate/
├── Cargo.toml           # Manifest and dependencies
├── src/
│   ├── lib.rs           # Library root (or main.rs for binaries)
│   ├── module.rs        # Public modules
│   └── internal/        # Private implementation details
├── tests/               # Integration tests
│   └── integration_test.rs
└── benches/             # Benchmarks
    └── benchmark.rs
```

## Common Patterns

### Error Handling

- **thiserror** — Derive macros for library error types
- **anyhow** — Ergonomic error handling for applications
- Prefer `Result<T, E>` over panics in library code

### Async Runtime

- **tokio** — Async runtime for I/O-bound workloads
- **async-std** — Alternative async runtime (choose one per project)

### Serialization

- **serde** — Serialization/deserialization framework
- **serde_json** — JSON support

### Terminal UI (tddy-coder)

- **ratatui** — TUI framework for terminal layout and widgets (activity log, status bar, prompt bar)
- **crossterm** — Cross-platform terminal manipulation (raw mode, alternate screen, key events)

### Logging & Observability

- **tracing** — Structured logging and instrumentation
- **tracing-subscriber** — Log output configuration

## Development Environment

- **Toolchain**: Stable Rust (`rustup default stable`)
- **Format on save**: `cargo fmt` before commit
- **Lint**: `cargo clippy` with `-D warnings` in CI
- **Documentation**: `cargo doc --open` for API docs

## Development Environment (Nix)

This project uses **Nix** for a reproducible development environment. Enter the shell:

```bash
nix develop
```

With **direnv**: add `use flake` to `.envrc`, run `direnv allow` once, and the environment loads automatically when you `cd` into the project.

The dev shell provides: `rustc`, `cargo`, `rustfmt`, `clippy`, `rust-analyzer`.

## Commands Reference

| Action | Command |
|--------|---------|
| Build | `cargo build` |
| Build (release) | `cargo build --release` |
| Test | `cargo test` |
| Test (doc tests) | `cargo test --doc` |
| Lint | `cargo clippy` |
| Format | `cargo fmt` |
| Check (no build) | `cargo check` |
| Doc | `cargo doc --open` |
| Run | `cargo run` |
