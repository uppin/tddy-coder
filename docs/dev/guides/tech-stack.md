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
│   ├── tddy-core/          # Core library (Presenter, Workflow, backends, changeset)
│   ├── tddy-tui/            # TUI View layer (ratatui, key_map, event_loop)
│   ├── tddy-coder/         # CLI binary (shared run logic with tddy-demo)
│   ├── tddy-demo/           # Demo binary (StubBackend, same TUI as tddy-coder)
│   └── tddy-permission/    # Permission MCP server
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

### Terminal UI (tddy-tui)

- **ratatui** — TUI framework for terminal layout and widgets (activity log, status bar, prompt bar)
- **crossterm** — Cross-platform terminal manipulation (raw mode, alternate screen, key events)
- **tddy-tui** — Separate package implementing `PresenterView`; maps keys to `UserIntent`, holds view-local state; tddy-coder and tddy-demo depend on it for TUI mode

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

### Per-machine Cargo config

The committed `.cargo/config.toml` holds **only project-wide, portable settings** (currently the macOS `-ObjC` rustflags that libwebrtc requires). Anything tied to a specific machine — absolute paths, a local crates.io proxy — must **not** be committed, because it breaks every other clone and OS (a hard-coded `/Users/...` path, for example, makes the libwebrtc native build fail on Linux/CI).

Put machine-specific settings in your **per-machine `~/.cargo/config.toml`** instead. Cargo merges it with the project config automatically and it is never part of the repo. Typical contents:

```toml
# ~/.cargo/config.toml — never committed

# Pre-extracted libwebrtc binaries for the livekit Rust SDK, so the native build
# skips the GitHub download. The tag must match webrtc-sys-build's WEBRTC_TAG
# (currently webrtc-51ef663). Point at the dir for YOUR platform:
[env]
# macOS:  LK_CUSTOM_WEBRTC = "/Users/<you>/.local/share/livekit-webrtc/mac-arm64-release"
# Linux:  LK_CUSTOM_WEBRTC = "/home/<you>/.local/share/livekit-webrtc/linux-x64-release"
LK_CUSTOM_WEBRTC = "/home/<you>/.local/share/livekit-webrtc/linux-x64-release"

# Optional: a local crates.io caching proxy, if you run one.
# [source.crates-io]
# replace-with = "crates-io-proxy"
# [source.crates-io-proxy]
# registry = "sparse+http://127.0.0.1:3080/index/"
```

Without `LK_CUSTOM_WEBRTC`, `webrtc-sys` downloads libwebrtc from the livekit GitHub release on the first build (needs network); with it, the build reuses the local copy.

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
