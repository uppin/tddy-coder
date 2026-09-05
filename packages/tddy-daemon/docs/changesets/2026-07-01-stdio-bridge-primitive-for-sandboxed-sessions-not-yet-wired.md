# 2026-07-01 — stdio bridge primitive for sandboxed sessions (not yet wired into real call sites)

**Type:** Feature

new `sandbox_session::bridge_sandbox_stdio` converts a jail-spawned `SandboxHandle`'s piped (blocking) stdio into an async RPC endpoint via `tokio::net::unix::pipe` + a new `tddy_stdio::StdioEndpoint::from_duplex` constructor; proven end-to-end through a real Seatbelt jail, including a full `run_host_relay` tool-call round trip via the new `StdioSandboxClient` (`tddy-sandbox-runner`). **`connection_service.rs`'s spawn/dial orchestration and `dial_and_bridge` are unchanged** — real sessions still spawn with `--grpc-uds`/`--grpc-listen-port` and dial the tonic client; wiring the daemon's actual call sites onto this primitive is tracked in `docs/dev/TODO.md`. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#253](https://github.com/uppin/tddy-coder/pull/253). (tddy-daemon, tddy-sandbox, tddy-sandbox-darwin, tddy-sandbox-runner, tddy-stdio)
