# 2026-07-01 — `--stdio` remote-control transport

**Type:** Feature

`tddy-coder`/`tddy-demo` gain a `--stdio` flag serving the existing `TddyRemote` remote-control surface over `tddy-stdio` (in addition to, not instead of, `--grpc` — both run concurrently on the same `PresenterHandle`); local TUI is skipped entirely under `--stdio` (fd 1 is dedicated to RPC framing). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#253](https://github.com/uppin/tddy-coder/pull/253). (tddy-coder, tddy-core, tddy-service, tddy-stdio)
