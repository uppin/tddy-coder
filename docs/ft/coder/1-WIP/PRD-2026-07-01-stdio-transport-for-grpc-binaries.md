# PRD: stdio RPC Transport for gRPC-Hosting Binaries

**Created:** 2026-07-01
**Product Area:** coder
**Status:** WIP

## Summary

`tddy-coder`, `tddy-demo`, and `tddy-sandbox-runner` each host their own ad hoc gRPC/TCP-or-UDS server. Add a `--stdio` flag to each, backed by a reusable "stdio-safe core," so any of them can be driven over stdin/stdout using the already-shipped `tddy-rpc`/`tddy-stdio` protocol instead of network sockets — and retire the sandbox tool-IPC protocol and the sandbox gRPC-over-UDS relay, which exist for the same category of problem without going through `tddy-rpc`.

## Background

Recent work ([rpc-multi-transport.md](../rpc-multi-transport.md), shipped 2026-07-01) gave `tddy-rpc` a transport-agnostic dispatch core (`client_engine::ClientEngine`, `server_engine::ServerEngine<S>`, `RpcClientTransport`) and added `tddy-stdio` as a second transport for parent↔child Rust processes, proven via `tddy-livekit`'s client refactor. That work didn't touch any of tddy's actual gRPC-hosting binaries or the sandbox's legacy IPC.

**Three binaries currently host a gRPC server of their own:**

- `tddy-coder`/`tddy-demo` (share `run.rs`'s `Args`/`CoderArgs`/`DemoArgs`): `--grpc <port>` starts a `tonic::transport::Server` for remote control ([grpc-remote-control.md](../grpc-remote-control.md)).
- `tddy-sandbox-runner`: hosts `SandboxServiceServer` (PTY I/O, session control) bound over `--grpc-uds` (Linux — required, since the sandbox's own network namespace blocks host loopback TCP) or `--grpc-listen-port` (macOS Seatbelt fallback — fixed-port TCP is simpler to allow via SBPL than a bind-mounted UDS path there). The host (`tddy-daemon`) connects back as a gRPC client after polling a `ready_marker` file for the bound address.

**Two bespoke JSON-over-Unix-socket protocols exist for the same kind of problem** (a host and a sandboxed/child process talking to each other) without going through `tddy-rpc` at all:

- **Sandbox tool-IPC** (`tddy-sandbox/src/tool_ipc.rs`, `tddy-sandbox-runner`'s `start_tool_ipc_server`, client in `tddy-tools/src/session_tool_client.rs::dispatch_via_sandbox_ipc`): forwards MCP tool calls from `tddy-tools` (running inside the jail) to the daemon. Framing is a single `read()`/`write_all()` of a whole JSON blob — no length prefix, no multi-read loop — so a payload that doesn't arrive in one syscall is silently truncated. Selected via `TDDY_SANDBOX_TOOL_IPC` env var / `--tool-ipc-socket` flag in `session_tool_client.rs::detect_session_tool_transport()`.
- **Toolcall listener** (`tddy-core/src/toolcall/listener.rs`) is a *third*, unrelated bespoke protocol between `tddy-coder` and the Claude Code CLI subprocess it spawns (`submit`/`ask`/`approve`/`list-actions`/`build` over newline-delimited JSON). **Out of scope for this change** — see [Non-goals](#non-goals) and the corresponding entry in `docs/dev/TODO.md`.

`SandboxServiceServer`'s proto (`tddy-service/proto/sandbox.proto`) is already codegen'd via `tddy-codegen` with `generate_rpc_server: true` (`tddy-service/build.rs`) — an `RpcService` implementation for it already exists and is unused today; only the tonic/UDS/TCP transport is wired up in `tddy-sandbox-runner/src/runner.rs`. Wiring `--stdio` for this binary is largely "point the existing generated `RpcService` at `ServerEngine` + `StdioEndpoint`," not new codegen.

**None of the three binaries' stdio streams are safe to reuse as an RPC channel today.** `tddy_core::init_tddy_logger` defaults logging to stderr (or a mute buffer under `TDDY_QUIET`) but accepts a user-configured `LogOutput::Stdout`. `tddy-coder`'s "plain mode" fallback (`plain.rs`) both `println!`s and reads stdin directly for clarification prompts. Ratatui/crossterm normally owns real fd 1 for TUI rendering. `tddy_stdio::StdioEndpoint` assumes the peer's fd 1 is *entirely* dedicated to length-prefixed frame I/O, with zero tolerance for stray bytes — so a stdio-safe core is required, not optional.

Two existing building blocks make this tractable: `--daemon` mode already redirects stderr to a real log file via `dup2` (so crossterm APIs still work) while leaving stdin/stdout untouched (`run.rs`, headless dispatch before TUI setup); and `tddy-tui`'s `CapturingWriter::headless()` already lets a Virtual TUI render without ratatui ever touching physical fd 1 (used today for LiveKit/daemon streaming).

## Requirements

### Functional Requirements

- [ ] A reusable **stdio-safe core** (new module, most likely in `tddy-core`) that, when a process is about to run in `--stdio` mode: redirects stderr to a log file (reusing the `--daemon` `dup2` pattern), force-overrides any configured `LogOutput::Stdout` to file/stderr, and is invoked before any TUI/plain-mode dispatch — so `plain.rs`'s direct stdin/stdout usage can never run concurrently with the RPC framing on the same fds.
- [ ] `tddy-coder` and `tddy-demo` gain a `--stdio` flag that serves the existing remote-control `RpcService` surface over a `tddy_stdio::StdioEndpoint`, in addition to (not instead of) `--grpc`'s `tonic::transport::Server` — the two transports can run concurrently, same as `--grpc` already coexists with local TUI/plain mode today; `--stdio` is exclusive only with TUI/plain-mode dispatch (fd 1 can't be shared between RPC framing and terminal rendering). Any local view rendering under `--stdio` goes through `CapturingWriter::headless()`, never physical fd 1.
- [ ] `tddy-sandbox-runner` gains a `--stdio` flag that serves `SandboxService` via its already-codegen'd `RpcService` impl over `ServerEngine`/`StdioEndpoint`, replacing `--grpc-uds`/`--grpc-listen-port`. `tddy-daemon` spawns it with `--stdio` and talks to it via `tddy_stdio::spawn_child_endpoint`, removing the `ready_marker` polling handshake, the UDS-vs-TCP per-platform branching, `pick_free_loopback_port`, and the `SUN_LEN` path-length hazard entirely.
- [ ] Sandbox tool-IPC (`tddy-tools` ↔ daemon/sandbox-runner MCP tool-call forwarding) is migrated onto `tddy-rpc`/`tddy-stdio`, replacing the unframed single-`read()`/`write_all()` JSON protocol. `session_tool_client.rs`'s existing transport-selection abstraction (`detect_session_tool_transport`) gains a stdio-RPC variant alongside its existing daemon-HTTP (Connect-RPC) variant.
- [ ] `--tool-ipc-socket`, `TDDY_SANDBOX_TOOL_IPC`, `--grpc-socket`, `--grpc-uds`, `--grpc-listen-port` and their handshake/allocation code (`pick_free_loopback_port`, `ready_marker` polling, `short_ipc_socket_path`/`SUN_LEN` workaround) are removed once the stdio path replaces them. No dual-path fallback is kept — per this repo's judgment boundary against unrequested fallbacks.

### Non-Functional Requirements

- [ ] No stray bytes ever reach physical fd 1 while `--stdio` is active, on any of the three binaries, under normal operation, logging misconfiguration, and panics (panics already only write to stderr today — confirmed, not a new risk).
- [ ] `SandboxService`'s existing PTY/session-control behavior is unchanged in substance — only its transport moves.
- [ ] No new external dependencies — the required RPC/stdio machinery already exists in `tddy-rpc`/`tddy-stdio`.

## Non-goals

- No changes to the LiveKit or Connect-RPC transports.
- No migration of the toolcall listener (`tddy-core/src/toolcall/listener.rs`, `tddy-coder` ↔ spawned Claude Code CLI subprocess) — tracked as a follow-up idea in `docs/dev/TODO.md`.
- No TypeScript/Node stdio transport (unchanged from `rpc-multi-transport.md`'s existing non-goal).

## Acceptance Criteria

- [ ] `tddy-coder --stdio` (and `tddy-demo --stdio`) can be driven end-to-end over stdin/stdout by a `tddy-stdio` client: send a `SubmitFeatureInput` intent, receive `PresenterView` events, with zero non-frame bytes ever observed on the child's stdout.
- [ ] `tddy-daemon` spawns `tddy-sandbox-runner --stdio` and drives `SandboxService` (PTY I/O, session control) entirely over the stdio pipe pair, with no `--grpc-uds`/`--grpc-listen-port`/`ready_marker` code path remaining.
- [ ] A tool call made by `tddy-tools` running inside the sandbox reaches the daemon and returns a result via the stdio RPC path, with no `--tool-ipc-socket`/`TDDY_SANDBOX_TOOL_IPC` code path remaining.
- [ ] A payload larger than one socket read (regression test for the truncation risk the old tool-IPC framing had) round-trips correctly over the new path.
- [ ] `cargo clippy -- -D warnings` and `./test` pass for all affected packages.

## Affected Features

- [grpc-remote-control.md](../grpc-remote-control.md) — `--stdio` becomes an alternative to `--grpc` for `tddy-coder`/`tddy-demo`; the "Transport stack" reference section gains `tddy-stdio`.
- [rpc-multi-transport.md](../rpc-multi-transport.md) — this PRD is the first real consumer of that transport beyond `tddy-livekit`.
