# 2026-07-01 — **`StdioEndpoint::from_duplex`

**Type:** Feature

wrap an already-open duplex stream** — new public constructor for hosting an `RpcService` over any `AsyncRead`/`AsyncWrite` pair the caller already owns (not just a `tokio::process::Command` `spawn_child_endpoint` spawns itself) — needed for jailed/sandboxed process stdio, which platform-specific spawn code (Seatbelt `sandbox-exec`, Linux namespaces) must own the spawning of. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#253](https://github.com/uppin/tddy-coder/pull/253). (tddy-stdio, tddy-sandbox-runner, tddy-daemon, tddy-tools)
