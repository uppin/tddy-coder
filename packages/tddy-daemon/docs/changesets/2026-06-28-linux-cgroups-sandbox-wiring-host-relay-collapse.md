# 2026-06-28 — Linux cgroups sandbox wiring + host-relay collapse

**Type:** Feature

`spawn_sandbox_runner` dispatches darwin/cgroups by target OS; `dial_and_bridge` collapsed onto shared `tddy_sandbox_runner::run_host_relay` (`DaemonToolHandler`), dials in-jail gRPC over AF_UNIX (`connect_sandbox_client_uds`) on Linux; runner argv gains `--grpc-uds`; dep `tddy-sandbox-cgroups` (linux). Acceptance: `sandboxed_claude_cli_starts_on_linux_with_the_cgroups_backend`. Architecture [tddy-sandbox](../../../../packages/tddy-sandbox/docs/architecture.md). (tddy-daemon, tddy-sandbox-runner, tddy-sandbox-cgroups)
