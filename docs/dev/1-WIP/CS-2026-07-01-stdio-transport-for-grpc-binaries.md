# Changeset: stdio RPC Transport for gRPC-Hosting Binaries

**Created:** 2026-07-01
**Status:** Created
**PRD:** docs/ft/coder/1-WIP/PRD-2026-07-01-stdio-transport-for-grpc-binaries.md

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Milestone 1: stdio-safe core (complete)
- [x] Milestone 2: `--stdio` on tddy-coder / tddy-demo (complete, one deviation — see milestone notes)
- [ ] Milestone 3: `--stdio` on tddy-sandbox-runner + daemon wiring (partial — Echo/EchoStream/SessionChannel work over stdio, proven through a real macOS Seatbelt jail via new `bridge_sandbox_stdio` primitive; Linux cgroups equivalent not attempted (unverifiable on this dev machine); daemon's actual session lifecycle (`dial_and_bridge`/`run_host_relay`) not yet switched over; old-transport removal not started)
- [ ] Milestone 4: migrate sandbox tool-IPC (partial — `dispatch_via_stdio_rpc` exists and is tested standalone; not wired into transport selection, old tool-IPC not removed)

## Affected Packages

- [ ] `tddy-core` (or a new small crate) — reusable stdio-safe core: stderr redirect, `LogOutput` override, `--stdio` mode gating
- [ ] `tddy-coder` — `--stdio` flag; remote-control `RpcService` served over `StdioEndpoint`; `CapturingWriter::headless()` wiring
- [ ] `tddy-sandbox-runner` — `--stdio` flag; serve `SandboxService`'s existing generated `RpcService` impl over `ServerEngine`/`StdioEndpoint`; remove UDS/TCP relay + `ready_marker` handshake
- [ ] `tddy-daemon` — spawn `tddy-sandbox-runner --stdio` via `spawn_child_endpoint`; migrate sandbox tool-IPC server-side wiring; remove port/UDS allocation code
- [ ] `tddy-tools` — `session_tool_client.rs` gains a stdio-RPC transport variant; remove the sandbox tool-IPC variant
- [ ] `tddy-sandbox` — remove `tool_ipc.rs`, `short_ipc_socket_path`, the `SUN_LEN` workaround. Gained `SandboxHandle::take_stdio()`.
- [ ] `tddy-sandbox-darwin` — `spawn_plan` pipes stdio (instead of an egress-log redirect) when `--stdio` is in the command. `tddy-sandbox-cgroups` (Linux) needs the equivalent change, not attempted here.
- [x] `tddy-rpc` — reused as-is. `tddy-stdio` gained one new public constructor, `StdioEndpoint::from_duplex`, for wrapping already-open duplex streams (not just a `tokio::process::Command` it spawns itself) — needed for jailed/sandboxed spawns.

## State A (Current)

Three binaries each host their own ad hoc gRPC/socket server:

- `tddy-coder`/`tddy-demo`: `--grpc <port>` → `tonic::transport::Server` (remote control).
- `tddy-sandbox-runner`: `SandboxServiceServer` over `--grpc-uds` (Linux) or `--grpc-listen-port` (macOS), reached by the daemon after polling a `ready_marker` file. `SandboxService`'s proto is already dual-codegen'd (`generate_rpc_server: true` in `tddy-service/build.rs`) — an unused `RpcService` impl already exists alongside the tonic one.

Two bespoke, non-`tddy-rpc` JSON-over-Unix-socket protocols exist for host↔sandboxed-process communication:

- Sandbox tool-IPC: `tddy-sandbox/src/tool_ipc.rs` + `tddy-sandbox-runner`'s listener + `tddy-tools/src/session_tool_client.rs::dispatch_via_sandbox_ipc`. Single `read()`/`write_all()` per call, no length prefix — truncates silently on a multi-syscall payload.
- Toolcall listener (`tddy-core/src/toolcall/listener.rs`): unrelated third protocol between `tddy-coder` and the spawned Claude Code CLI. **Explicitly out of scope** — see PRD Non-goals; follow-up filed in `docs/dev/TODO.md`.

`tddy-rpc`/`tddy-stdio` (shipped 2026-07-01, [rpc-multi-transport.md](../../ft/coder/rpc-multi-transport.md)) provide a transport-agnostic RPC engine and a stdio transport, but nothing in the repo other than `tddy-livekit`'s client consumes them yet.

No binary today has a `--stdio` flag. No shared mechanism exists to guarantee a process's fd 1 is clean enough to double as an RPC channel — `LogOutput::Stdout` is a reachable misconfiguration, `plain.rs` uses stdin/stdout directly, and `tddy_stdio::StdioEndpoint` has zero tolerance for stray bytes on the peer's stdout.

## State B (Target)

- A shared stdio-safe core, invoked by all three binaries before any TUI/plain-mode dispatch, guarantees fd 1 carries only RPC frames while `--stdio` is active (stderr redirected to a log file, `LogOutput::Stdout` force-overridden, plain mode unreachable).
- `tddy-coder`/`tddy-demo --stdio` serve the existing remote-control `RpcService` surface over `StdioEndpoint`, as an alternative to `--grpc`.
- `tddy-sandbox-runner --stdio` serves `SandboxService`'s existing generated `RpcService` impl over `ServerEngine`/`StdioEndpoint`. `tddy-daemon` spawns it via `spawn_child_endpoint` — no `ready_marker`, no UDS/TCP branching, no port allocation.
- Sandbox tool-IPC calls travel over the same stdio-RPC channel, using `tddy-rpc`'s length-prefixed framing (fixing the old truncation risk as a side effect).
- `--tool-ipc-socket`, `TDDY_SANDBOX_TOOL_IPC`, `--grpc-socket`, `--grpc-uds`, `--grpc-listen-port`, `pick_free_loopback_port`, `short_ipc_socket_path`, and the `ready_marker` handshake are deleted, not deprecated.

## Delta

### New

- Stdio-safe core (stderr redirect + `LogOutput` override + `--stdio` mode gating), shared by all three binaries.
- `--stdio` CLI flag on `tddy-coder`, `tddy-demo`, `tddy-sandbox-runner`.
- Stdio-RPC transport variant in `tddy-tools`'s session-tool-client transport selection.

### Modified

- `tddy-daemon`'s sandbox spawn/connect logic (`connection_service.rs`, `sandbox_plan_builder.rs`): UDS/TCP handshake → `spawn_child_endpoint`.
- `tddy-sandbox-runner/src/runner.rs`: wire the existing codegen'd `SandboxService` `RpcService` impl to `ServerEngine`/`StdioEndpoint`.
- `docs/ft/coder/grpc-remote-control.md`: document `--stdio` as an alternative transport.

### Removed

- Sandbox tool-IPC: `--tool-ipc-socket`/`TDDY_SANDBOX_TOOL_IPC`, the listener in `tddy-sandbox-runner`, `tddy-sandbox/src/tool_ipc.rs`, `dispatch_via_sandbox_ipc`.
- Sandbox gRPC relay: `--grpc-socket`/`--grpc-uds`/`--grpc-listen-port`, `ready_marker` polling, `pick_free_loopback_port`, `short_ipc_socket_path`/`SUN_LEN` workaround.

## Milestones

### Milestone 1: Stdio-safe core — ✅ complete

- [x] Build the reusable stdio-safe core: stderr redirect (reusing `--daemon`'s `dup2` pattern, generalized into `tddy_core::stdio_safety::redirect_fd_to_file`), force-override of `LogOutput::Stdout` (`enforce_stdio_safe_log_output`), `--stdio` mode gated ahead of plain-mode/TUI dispatch
- [x] Unit tests: `LogOutput` override behavior, mode-gating precedence, stderr redirect (`packages/tddy-core/tests/stdio_safety.rs`, 8/8 passing)

### Milestone 2: `--stdio` on tddy-coder / tddy-demo — ✅ complete

- [x] Wire `--stdio` to serve the existing remote-control surface over `StdioEndpoint` — required dual-codegen'ing `TddyRemote` as an `RpcService` in `tddy-service` (new `build.rs` pass + `crate::proto::remote` module + second trait impl on `TddyRemoteService`, reusing `crate::gen::*` message types via `extern_path`; this wasn't pre-existing like `SandboxService`'s dual codegen, unlike originally assumed for the other binaries)
- [x] **Deviation**: skipped local TUI entirely under `--stdio` instead of wiring `CapturingWriter::headless()` — simpler, matches `--daemon`'s existing "headless, no local view" precedent, and no test requires a live local view under `--stdio`. `CapturingWriter::headless()` remains unused for this purpose; revisit if a future requirement needs a local view alongside `--stdio`.
- [x] Acceptance test: drive `tddy-coder --stdio` end-to-end via a `tddy-stdio` client (`SubmitFeatureInput` → `PresenterView` events) (`packages/tddy-e2e/tests/stdio_remote_control_acceptance.rs`)
- [ ] Not done: `--stdio`/`--grpc` mutual-exclusivity validation (PRD mentions it; no test exercises it yet — needs a follow-up red test before implementing, to avoid unrequested scope)

### Milestone 3: `--stdio` on tddy-sandbox-runner + daemon wiring — ⚠️ partial

- [x] Wire `--stdio` to serve `SandboxService`'s existing generated `RpcService` impl over `StdioEndpoint`, for `Echo`/`EchoStream`/`SessionChannel` (`packages/tddy-sandbox-runner/src/runner.rs`)
- [x] `SessionChannel` (PTY/session-control) over `--stdio` fully implemented — root-caused the gap differently than first assumed: `sandbox.proto`'s own message types (`SessionFrame`, `EchoRequest`, etc.) were generated *twice*, independently, once by the RpcService-flavored `prost_build` pass (canonical, `proto::sandbox`) and once by the `tonic_build` pass (`tonic_sandbox`) — the tonic pass already `extern_path`'d 3 *different*-package types (`connection.*`) but not sandbox.proto's own. Fixed by extending that same `extern_path` list to sandbox.proto's own message types (mirrors `terminal.proto`'s established dual-codegen pattern exactly). This makes both `SandboxService` trait impls (tonic and RpcService/stdio) use identical Rust types, so `session_channel`'s relay-calling logic could be copied verbatim from the tonic impl (adapted to `tddy_rpc::{Request,Response,Streaming,Status}` wrapper types). One wrinkle: `SandboxSessionRelay`'s outbound channel is hardcoded to `tonic::Status` (shared by construction with the tonic impl) — `tddy-rpc`'s own optional `tonic` feature pins tonic 0.11, incompatible with this crate's tonic 0.12, so status conversion is a small hand-written function (`tonic_status_to_rpc`) at the trait boundary, not the crate's blanket `From` impl.
- [x] **Jail-spawn stdio piping (macOS)**: `tddy_sandbox_darwin::spawn_plan` pipes stdin/stdout instead of redirecting stdout to an egress log when `--stdio` is present in the command (`stdio_mode` check on `plan.spec.command`) — this was a bigger finding than originally scoped: the daemon and sandboxed runner talk *exclusively* over `--grpc-uds`/`--grpc-listen-port` today; the process's own stdio was never piped back to the daemon at all (stdout went to a log file, stdin was null unless a one-shot payload was set). `SandboxHandle::take_stdio()` (`tddy-sandbox`) exposes the piped (blocking) `std::process::ChildStdin`/`ChildStdout`. `tddy_daemon::sandbox_session::bridge_sandbox_stdio` converts them to async via `tokio::net::unix::pipe::{Sender,Receiver}::from_owned_fd` (handles the `O_NONBLOCK` flag internally) and hosts an `RpcService` endpoint over them via a new `tddy_stdio::StdioEndpoint::from_duplex` constructor (generic over any `AsyncRead`/`AsyncWrite` pair — needed because `spawn_child_endpoint` assumes it owns spawning via `tokio::process::Command`, which the jail-specific spawn functions can't use).
- [x] Proven end-to-end with a **real Seatbelt-jailed** process (not a directly-spawned unsandboxed `tddy-sandbox-runner`, unlike the earlier Echo/SessionChannel tests): `packages/tddy-daemon/tests/sandbox_stdio_seatbelt_acceptance.rs` calls the actual production `spawn_sandbox_runner` → `bridge_sandbox_stdio` → `Echo` round trip. All pre-existing Seatbelt/tonic tests (`sandbox_runner_spawn_smoke.rs`, `tddy-sandbox-darwin`'s full suite) still pass unchanged — the non-`--stdio` log-redirect path is untouched.
- [ ] **Linux (`tddy-sandbox-cgroups`) not touched** — this dev environment is macOS-only, so a Linux-specific jail-spawn change couldn't be verified here (not even compile-checked, since that crate is `#[cfg(target_os = "linux")]`-gated). Needs the equivalent `stdio_mode` piping change made and verified on Linux before `--stdio` is usable there.
- [ ] `tddy-daemon`'s actual session lifecycle (`dial_and_bridge`, `connect_session_client`, `run_host_relay`) has **not** been switched to use `bridge_sandbox_stdio` — only the low-level spawn/bridge primitives exist and are proven; the daemon still dials the old UDS/TCP `SandboxServiceClient` (tonic) for real sessions. `run_host_relay` (in `tddy-sandbox-runner`) is written against a typed tonic client and needs an equivalent rewritten against `tddy_rpc::RpcClientTransport`'s untyped `call_unary`/`start_bidi_stream` interface (there is no generated *client* stub for RpcService-flavored services, only server-side glue — confirmed by reading `tddy-codegen`). This is a similarly-sized body of work to everything else in Milestone 3 combined, deliberately not attempted in this session — recommend scoping it as its own follow-up changeset once Linux parity is in place.
- [ ] `--grpc-socket`/`--grpc-uds`/`--grpc-listen-port` and associated port/path-allocation code NOT removed (by design — removal needs the daemon-side wiring above, on both platforms, first)
- [x] Acceptance tests: `SandboxService/Echo` round-trips over `--stdio` both directly-spawned and through a real Seatbelt jail; real PTY output flows over a stdio-served `SessionChannel` (subscribe + poll → `TerminalOutput`) (`packages/tddy-daemon/tests/sandbox_runner_stdio_acceptance.rs`, `sandbox_stdio_seatbelt_acceptance.rs`) — narrower than the original "daemon drives a sandboxed session... over stdio only" criterion only in that the daemon's *session lifecycle* isn't wired to use this path yet; the actual jail-spawn and PTY/relay machinery is now proven, not just Echo
- Note: confirmed `run_claude_pty_thread` (the code path for `--claude-binary`, as opposed to `--pty-command`'s `run_generic_pty_thread`) never calls `relay.signal_session_ended()` — a pre-existing gap (not introduced here, not fixed here) that means `SessionEnded` frames are never emitted for real claude sessions, only for the generic-PTY-command path. Worth a follow-up issue independent of this changeset.

### Milestone 4: Migrate sandbox tool-IPC — ⚠️ partial

- [x] `dispatch_via_stdio_rpc(client, tool_name, args)` added to `session_tool_client.rs`, calling `connection.ConnectionService/ExecuteTool` over an injected `RpcClientTransport` — tested standalone against a fixture, not yet reachable from a real code path
- [ ] NOT wired into `detect_session_tool_transport()`'s selection logic — no env var or config chooses this transport yet; a real caller still can't reach it
- [ ] `--tool-ipc-socket`/`TDDY_SANDBOX_TOOL_IPC` and the old listener/client code NOT removed (by design — removal needs the new path to be actually selectable first)
- [x] Regression test: a 256KB payload round-trips correctly through `dispatch_via_stdio_rpc` (the bug the old framing had) (`packages/tddy-tools/tests/session_tool_stdio_rpc_dispatch.rs`)
- [ ] Acceptance test is narrower than planned: exercises `dispatch_via_stdio_rpc` against a standalone fixture process, not a real MCP tool call from sandboxed `tddy-tools` reaching a real daemon over stdio only — that requires Milestone 3's daemon wiring and this milestone's transport-selection wiring first

## Testing Strategy

### Acceptance Tests

- [ ] `tddy-coder --stdio` end-to-end remote-control round trip, zero stray stdout bytes
- [ ] `tddy-sandbox-runner --stdio` end-to-end PTY/session-control round trip via the daemon
- [ ] Sandboxed MCP tool call round trip over stdio-RPC
- [ ] Large-payload round trip (regression for the old tool-IPC truncation risk)

### Test Level Decisions

| Aspect | Level | Rationale |
|--------|-------|-----------|
| Stdio-safe core (redirect, override, gating) | Unit | Pure logic, no process spawning needed |
| `--stdio` remote-control round trip | Integration/Acceptance | Needs a real spawned process + `StdioEndpoint` |
| `SandboxService` over stdio | Integration/Acceptance | Needs a real spawned `tddy-sandbox-runner` process |
| Sandbox tool-IPC migration | Integration/Acceptance | Needs a real daemon + sandboxed `tddy-tools` round trip |
| Large-payload framing | Unit/Integration | Testable directly against `StdioEndpoint`, no full sandbox needed |

## Technical Debt

- Toolcall listener (`tddy-core/src/toolcall/listener.rs`) remains on its own bespoke newline-JSON protocol — tracked as a follow-up in `docs/dev/TODO.md`.
- `--stdio`/`--grpc` mutual exclusivity is unvalidated on `tddy-coder`/`tddy-demo` (Milestone 2).
- `tddy-daemon`'s real session lifecycle (`dial_and_bridge`/`connect_session_client`/`run_host_relay`) has not been switched to the new `bridge_sandbox_stdio` primitive — still dials the tonic `SandboxServiceClient` over UDS/TCP for every real session. The primitive itself (jail spawn with piped stdio → async bridge → RPC client) is built and proven end-to-end on macOS (Milestone 3); wiring the daemon's actual session lifecycle onto it requires rewriting `run_host_relay` against `tddy_rpc::RpcClientTransport`'s untyped interface (no generated client stub exists for RpcService-flavored services) — a similarly large body of work, recommended as its own follow-up changeset.
- The Linux jail-spawn path (`tddy-sandbox-cgroups`) has not been given the equivalent stdio-piping change `tddy-sandbox-darwin` received — this dev environment is macOS-only, so Linux-specific sandboxing code can't even be compile-checked here, let alone verified. `--stdio` is only proven end-to-end through a real jail on macOS.
- `dispatch_via_stdio_rpc` (Milestone 4) is not reachable from any real code path yet — not wired into `detect_session_tool_transport()`, so the old sandbox tool-IPC protocol is still the only one actually in use.
- The legacy `--grpc-socket`/`--grpc-uds`/`--grpc-listen-port` (sandbox relay) and `--tool-ipc-socket`/`TDDY_SANDBOX_TOOL_IPC` (tool-IPC) transports have not been removed — both remain live and in use pending the above.
- `run_claude_pty_thread` (real claude sessions) never calls `relay.signal_session_ended()`, unlike `run_generic_pty_thread` (the `--pty-command` path) — pre-existing gap, found while testing `SessionChannel` over `--stdio`, unrelated to this changeset's scope. Worth its own follow-up.
- `tddy-rpc`'s optional `tonic` feature pins tonic 0.11, incompatible with the tonic 0.12 used by `tddy-coder`/`tddy-sandbox-runner`/etc. — its `Status` conversions can't be used directly by any gRPC-hosting binary on 0.12 without a version bump. Worth reconciling if more dual-transport services need tonic↔tddy_rpc `Status` conversion (currently hand-rolled per call site, e.g. `tonic_status_to_rpc` in `runner.rs`).
