# 2026-11-02 — Workspace provisioner built per `<tddyhome>`

`#agent-tools` 1/12 ([#790](https://github.com/uppin/tddy-coder/pull/790)). `DaemonSessionHost` builds `JailedWorkspaceSandboxProvisioner::for_tddy_home(&tddy_data_dir)`, so declared tools load from `<tddyhome>/tools`. `svc_host_builders.rs` 500 → 503 production lines, recorded in `docs/code-issues/oversized-file-svc-host-builders.md`.
