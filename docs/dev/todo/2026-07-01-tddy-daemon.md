# 2026-07-01 — tddy-daemon

**Category:** Future enhancement
**Source:** stdio-transport-for-grpc-binaries changeset, 2026-07-01

- **Switch real session lifecycle onto the stdio transport** — spawn/dial for sandboxed sessions still uses gRPC UDS/listen-port in several paths. Primitives (`bridge_sandbox_stdio`, `StdioSandboxClient`, `run_host_relay`) are proven; wiring the daemon-side call sites was deferred while orchestration lived in `connection_service.rs`. **`#unbundle` node 9 (2026-09-10) deleted that file** — spawn/dial call sites now live in **`tddy-session-lifecycle`** (and `dial_and_bridge` remains in **`tddy-daemon-sandbox`**). The switch is still a live-behaviour change for every real session and is still not done; scope the work against those crates, not the removed facade.
- **Linux (`tddy-sandbox-cgroups`) jail-spawn stdio piping** — `tddy-sandbox-darwin::spawn_plan` was updated to pipe stdin/stdout (instead of redirecting stdout to an egress log) when `--stdio` is in the command; `tddy-sandbox-cgroups` needs the equivalent change on Linux. Not attempted in the original changeset because that crate is `#[cfg(target_os = "linux")]`-gated and couldn't even be compile-checked on the macOS dev environment that did the work, let alone verified through a real jail.
