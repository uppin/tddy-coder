# 2026-03-22 — Local web dev: `./web-dev`

- **Feature doc**: [local-web-dev.md](../local-web-dev.md) describes the daemon + Vite flow, **`DAEMON_CONFIG`**, temp YAML, CLI pass-through, **`DAEMON_PORT`** for the proxy, and **`fuser`** port cleanup.
- **E2E contract tests**: `packages/tddy-e2e` includes static checks for the repo-root **`web-dev`** script (`cargo test -p tddy-e2e web_dev`).
