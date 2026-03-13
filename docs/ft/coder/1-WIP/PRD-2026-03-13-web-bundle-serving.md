# PRD: Web Bundle Serving from tddy-coder / tddy-demo

**Status:** WIP
**Date:** 2026-03-13

## Summary

Add `--web-port <port>` and `--web-bundle-path <path>` CLI flags to `tddy-coder` and `tddy-demo` that start an HTTP static file server serving the pre-built `tddy-web` bundle. The binaries never depend on `tddy-web` at compile time — they simply serve whatever static files exist at the given path.

## Background

`tddy-web` is a React dashboard for dev progress tracking. It is built by `bun build src/index.tsx --outdir dist`, producing static JS/CSS/HTML assets in `packages/tddy-web/dist/`. Currently, there is no way to serve this bundle from the tddy-coder process. Users must start a separate HTTP server manually.

The TUI and daemon already run alongside gRPC and LiveKit services. Adding a static file HTTP server follows the same pattern: spawn an async task on the existing tokio runtime, bind to the user-specified port, and serve files from the user-specified path.

## Proposed Changes

### CLI Flags

Two new flags on `CoderArgs`, `DemoArgs`, and `Args`:

- `--web-port <PORT>` — Port for the HTTP static file server
- `--web-bundle-path <PATH>` — Path to the directory containing the built web assets (e.g. `packages/tddy-web/dist`)

Both flags must be provided together. If only one is present, the binary exits with an error.

### Static File Server

- Use `axum` with `tower-http`'s `ServeDir` to serve static files from `--web-bundle-path`
- The server binds to `0.0.0.0:<web-port>` and serves all files under the bundle path
- The server runs in a background tokio task, same as gRPC and LiveKit

### Modes

The web server works in:

- **TUI mode** — spawned alongside gRPC/LiveKit in `run_full_workflow_tui`
- **Daemon mode** — spawned in `run_daemon` alongside gRPC/LiveKit

### Code Location

The web server logic lives in `tddy-coder` (e.g. a `web_server` module in `packages/tddy-coder/src/`), alongside the existing gRPC and LiveKit server spawning in `run.rs`.

### No Build-Time Dependency

`tddy-coder` and `tddy-demo` do NOT depend on `tddy-web`. The `--web-bundle-path` is a runtime-only path to pre-built static assets. The user is responsible for building the web bundle before starting the binary.

## Affected Features

- [grpc-remote-control.md](../grpc-remote-control.md) — Daemon startup flow adds web server alongside gRPC
- [1-OVERVIEW.md](../1-OVERVIEW.md) — CLI flags documentation

## Dependencies (new external crates)

| Crate | Purpose |
|-------|---------|
| `axum` | HTTP framework for static file serving |
| `tower-http` (with `fs` feature) | `ServeDir` middleware for static file serving |

## Success Criteria

1. `tddy-demo --web-port 8080 --web-bundle-path packages/tddy-web/dist` serves the web dashboard at `http://localhost:8080/`
2. `tddy-coder --daemon --grpc --web-port 8080 --web-bundle-path packages/tddy-web/dist` runs gRPC + web server in daemon mode
3. Providing `--web-port` without `--web-bundle-path` (or vice versa) produces a clear error message
4. The web server serves `index.html` as the default for `/` and all files under the bundle path
5. Tests verify the web server starts, serves files, and validates flag combination errors
