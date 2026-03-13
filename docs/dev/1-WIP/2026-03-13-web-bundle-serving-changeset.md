# Changeset: Web Bundle Serving from tddy-coder / tddy-demo

**Date**: 2026-03-13
**Status**: ✅ Complete
**Type**: Feature

## Affected Packages

- **tddy-coder**: [README.md](../../packages/tddy-coder/README.md) - New CLI flags, web_server module

## Related Feature Documentation

- [PRD-2026-03-13-web-bundle-serving.md](../../docs/ft/coder/1-WIP/PRD-2026-03-13-web-bundle-serving.md)

## Summary

Add `--web-port` and `--web-bundle-path` CLI flags to tddy-coder and tddy-demo. When both are provided, an axum static file server runs alongside gRPC/LiveKit in both TUI and daemon modes.

## Background

tddy-web is a React dashboard built to `packages/tddy-web/dist/`. Users need to serve it from within the tddy-coder process. No compile-time dependency on tddy-web — runtime path only.

## Scope

- [x] **Implementation**: CLI flags, web_server module, validation, daemon + TUI wiring
- [x] **Testing**: 4 acceptance tests (flag validation, help, daemon serves index.html)
- [x] **Dependencies**: axum, tower-http (fs)

## Technical Changes

### State A (Before)

- No web serving capability
- CLI had no --web-port or --web-bundle-path

### State B (After)

- `Args`, `CoderArgs`, `DemoArgs` have `web_port: Option<u16>` and `web_bundle_path: Option<PathBuf>`
- `validate_web_args()` enforces both-or-neither
- `web_server::serve_web_bundle(port, path)` uses axum + ServeDir
- Daemon: tokio::spawn web server in rt.block_on
- TUI: std::thread::spawn with tokio runtime for web server

## Implementation Milestones

- [x] Add axum, tower-http to Cargo.toml
- [x] Create web_server.rs module
- [x] Add CLI flags to Args, CoderArgs, DemoArgs
- [x] Add validate_web_args, call from run_with_args
- [x] Wire web server in run_daemon
- [x] Wire web server in run_full_workflow_tui
- [x] Acceptance tests: flag validation, help, daemon serves files

## Acceptance Tests

- `web_port_alone_errors_with_clear_message` — --web-port without --web-bundle-path exits with error
- `web_bundle_path_alone_errors_with_clear_message` — --web-bundle-path without --web-port exits with error
- `help_shows_web_port_and_web_bundle_path` — --help documents both flags
- `daemon_with_web_flags_serves_index_html_at_root` — daemon with both flags serves index.html at /
