# 2026-07-01 — stdio RPC transport for gRPC-hosting binaries

- `tddy-coder`/`tddy-demo` gain a `--stdio` flag serving the existing `TddyRemote` remote-control surface over `tddy-stdio`, running concurrently with `--grpc` (not mutually exclusive — verified empirically, corrected from the original plan)
- `tddy-sandbox-runner` gains `--stdio` for its own `SandboxService` (`Echo`/`EchoStream`/`SessionChannel`, incl. real PTY output), proven end-to-end through a real macOS Seatbelt jail
- New `tddy_core::stdio_safety` module guarantees a `--stdio` process's fd 1 carries only RPC frames (log-output override + stderr redirect)
- `run_host_relay` (the actual production host-side session relay for sandboxed sessions) made transport-agnostic via a new `SessionChannelClient` trait — proven with a full tool-call round trip through a real jail via the new `StdioSandboxClient`, though `tddy-daemon`'s real session-lifecycle call sites aren't switched over yet
- Sandbox tool-IPC (`tddy-tools` ↔ `tddy-sandbox-runner`) migrated from an unframed JSON protocol to the same `tddy-rpc` framing, fixing a latent large-payload truncation risk
- Feature: [grpc-remote-control.md](../grpc-remote-control.md#stdio-transport), [rpc-multi-transport.md](../rpc-multi-transport.md#real-consumers-2026-07-01-follow-up)
