# 2026-03-13 — Web Bundle Serving

- **CLI flags**: `--web-port <PORT>` and `--web-bundle-path <PATH>` serve pre-built tddy-web static assets over HTTP. Both flags required together.
- **Modes**: Web server runs in TUI and daemon modes alongside gRPC/LiveKit.
- **Implementation**: axum + tower-http ServeDir; web_server module; validate_web_args for flag validation.
- **Packages**: tddy-coder (web_server.rs, run.rs wiring, acceptance tests).
