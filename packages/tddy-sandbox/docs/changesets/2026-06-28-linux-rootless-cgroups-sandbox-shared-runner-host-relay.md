# 2026-06-28 — Linux rootless cgroups sandbox + shared runner/host-relay

**Type:** Feature

new `tddy-sandbox-runner` crate (in-jail runner + `run_host_relay`, moved out of `tddy-sandbox-darwin`; AF_UNIX gRPC transport via `--grpc-uds`/`connect_sandbox_client_uds`) and `tddy-sandbox-cgroups` crate (rootless `spawn`: unprivileged userns + network ns with loopback-only egress + private mount ns + cgroup v2 limits; fails fast, no unconfined fallback). Darwin slimmed to spawn/profile + re-exports the runner; daemon/app/testing-commons collapsed onto `run_host_relay`. Architecture [architecture.md](../architecture.md). (tddy-sandbox, tddy-sandbox-runner, tddy-sandbox-cgroups, tddy-sandbox-darwin, tddy-sandbox-app, tddy-daemon, tddy-testing-commons, tddy-web)
