# 2026-10-24 — The ACP driver can serve an agent over a stream
**Type:** Feature

`AcpDriver::spawn_over(UnixStream, …)` serves an agent another process started (a `tddy-supervisor` relayed spawn); the worker takes `AgentIo::{Spawn, Relayed}` and both constructors share `start_worker` (`serve` 66 → 28 lines). `codex_acp_route_args` is exported (#721, `#coagents` 2/8). Code issue `oversized-file-backend-mod`: 906 → 907 (stack-shared, row added).
