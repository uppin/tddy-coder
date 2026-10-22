# 2026-10-22 — `--provider-route-path`

**Type:** Feature

`#provider-route` 4/9 ([#665](https://github.com/uppin/tddy-coder/pull/665)). `--provider-route-path`
(`Args` / `CoderArgs.provider_route_path`) loads an owner-only route file (`provider_route_arg.rs`) and
`create_backend` applies it to the backend; a specialized definition with a route is refused. Every
`create_backend` call site goes through `create_backend_for_args` or passes the route. Codex binary
resolution moved to `codex_binaries.rs`; `run.rs` 2689 → 2595 production lines.
