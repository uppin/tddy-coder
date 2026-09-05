# 2026-03-13 — Web Bundle Serving

**Type:** Feature

`--web-port` and `--web-bundle-path` CLI flags. When both provided, axum static file server runs alongside gRPC/LiveKit in TUI and daemon modes. validate_web_args enforces both-or-neither. web_server module with ServeDir. (tddy-coder)
