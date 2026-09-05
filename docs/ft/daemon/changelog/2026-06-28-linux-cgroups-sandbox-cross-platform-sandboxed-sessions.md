# 2026-06-28 — Linux cgroups sandbox + cross-platform sandboxed sessions

- Sandboxed `claude-cli` sessions now run on **Linux** via a rootless jail (`tddy-sandbox-cgroups`): unprivileged user namespace + network namespace (loopback-only egress, forcing the in-jail `HTTPS_PROXY`) + private mount namespace + cgroup v2 limits
- `spawn_sandbox_runner` dispatches darwin (Seatbelt) / linux (cgroups) by target OS; on Linux the in-jail gRPC `SessionChannel` is served over **AF_UNIX** (survives the network namespace), dialed via `connect_sandbox_client_uds`
- Fails fast with `failed_precondition` when the host lacks unprivileged user namespaces or a writable cgroup v2 subtree — no silent unconfined fallback (production daemon runs as root systemd, where the restriction doesn't apply)
- In-jail runner + host-side relay extracted to a shared `tddy-sandbox-runner` crate; the CONNECT egress shim now waits for the host to attach before relaying (fixes an early-tunnel race)
- Sandbox opt-in exposed in the tddy-web new-session form (the `tddy-tools pty-relay --sandbox` CLI flag already existed)
- Feature: [claude-cli-session.md](../claude-cli-session.md). Technical: [tddy-sandbox architecture](../../../../packages/tddy-sandbox/docs/architecture.md). Known follow-ups: `pivot_root` filesystem write-confinement, config-driven cgroup limits
