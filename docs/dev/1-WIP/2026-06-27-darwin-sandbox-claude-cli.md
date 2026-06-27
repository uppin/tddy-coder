# Changeset: Darwin-Sandboxed Claude Code CLI Sessions (Local gRPC)

**Date:** 2026-06-27  
**Packages:** `tddy-sandbox`, `tddy-sandbox-darwin`, `tddy-service`, `tddy-daemon`, `tddy-tools`, `tddy-core`, `tddy-testing-commons`, `tddy-workflow-recipes`  
**Feature PRD:** [docs/ft/1-WIP/PRD-2026-06-27-darwin-sandbox-claude-cli.md](../../ft/1-WIP/PRD-2026-06-27-darwin-sandbox-claude-cli.md)  
**Seatbelt spawn investigation:** [2026-06-27-darwin-sandbox-seatbelt-investigation.md](./2026-06-27-darwin-sandbox-seatbelt-investigation.md)  
**Agent skill:** [.agents/skills/darwin-sandbox/SKILL.md](../../../.agents/skills/darwin-sandbox/SKILL.md)

## Scope

- [x] **Package Documentation**: PRD + changeset + seatbelt investigation + darwin-sandbox skill ✅
- [x] **Implementation**: Full sandbox stack — spawn, SessionChannel, egress relay, MCP allowlist ✅
- [x] **Testing**: All sandbox acceptance tests green ✅
- [x] **Integration**: Daemon sandbox spawn + lifecycle (delete/resume) ✅
- [x] **Code Quality**: `cargo fmt`, `cargo clippy -- -D warnings` on sandbox packages ✅

## Architecture

**Single egress path:** outbound network from the jail is **`(deny network*)`**. The sandbox cannot dial out. The host daemon dials into the in-jail gRPC server; all external reachability (MCP tools, LLM HTTP) is relayed on the **host-poll `SessionChannel`**.

```
Client ──► tddy-daemon ConnectionService
              │
              │  dial_and_bridge: one SessionChannel loop
              ▼
         sandbox-runner (in-jail gRPC server, no outbound network)
              │  HostPoll-driven flush
              ├─► claude PTY → TerminalOutput
              ├─► MCP IPC queue → ExecuteToolRequest → host tool_engine
              └─► HTTP shim queue → EgressRequest → host outbound HTTP(S)
                                         ◄── EgressResponse
```

**`SessionChannel` frame model** (`packages/tddy-service/proto/sandbox.proto`):

| Direction | Frames |
|-----------|--------|
| Host → sandbox | `SubscribeTerminal`, `HostPoll`, `SandboxInput`, `ExecuteToolResponse`, `EgressResponse` |
| Sandbox → host | `SessionTerminalOutput`, `ExecuteToolRequest`, `EgressRequest` |

Debug probes retained: unary `Echo`, bidi `EchoStream`.

**Superseded designs (removed):**
1. Split RPCs: `StreamSandboxTerminalOutput` + `SandboxToolExecChannel`
2. Host loopback TCP/HTTP proxy + `HTTPS_PROXY` + `egress_proxy.rs`

## Milestones

- [x] M1: `tddy-sandbox` trait + context dir + Unsupported error
- [x] M2: `tddy-sandbox-darwin` Seatbelt impl + SBPL template
- [x] M3: `sandbox.proto` `SessionChannel` + `StartSessionRequest.sandbox` flag + codegen
- [x] M4: `tddy-tools sandbox-runner` + `SandboxSessionRelay` (host-poll queue)
- [x] M5: daemon `start_sandboxed_claude_cli_session` + `dial_and_bridge`
- [x] M6: `ResumeSession` / `DeleteSession` lifecycle (stop `SandboxHandle`, relaunch runner)
- [x] M7: non-darwin Unsupported → `failed_precondition`
- [x] M8: `EgressRequest` / `EgressResponse` on SessionChannel + in-jail HTTP shim + host relay
- [x] M8b: Seatbelt `(deny network*)` in production profile
- [x] M9: MCP allowlist in sandbox claude spawn (`sandbox_claude_spawn.rs`)
- [x] M10: Acceptance tests use SessionChannel egress (no HTTPS_PROXY / TCP proxy)

## Acceptance tests

| Test file | Status |
|-----------|--------|
| `packages/tddy-sandbox/tests/unsupported_on_non_darwin.rs` | ✅ |
| `packages/tddy-sandbox-darwin/tests/seatbelt_confinement_acceptance.rs` | ✅ 2/2 |
| `packages/tddy-tools/tests/sandbox_runner_acceptance.rs` | ✅ 4/4 |
| `packages/tddy-tools/tests/sandbox_runner_behavior_acceptance.rs` | ✅ 2/2 |
| `packages/tddy-daemon/tests/sandbox_behavior_acceptance.rs` | ✅ 5/5 |
| `packages/tddy-daemon/tests/sandboxed_claude_cli_acceptance.rs` | ✅ 4/4 |
| `packages/tddy-daemon/tests/sandboxed_session_lifecycle_acceptance.rs` | ✅ 2/2 |

## Implementation evidence

| Deliverable | Location |
|-------------|----------|
| Sandbox spec + context dir | `packages/tddy-sandbox/src/` |
| Seatbelt spawn + profile template | `packages/tddy-sandbox-darwin/src/spawn.rs`, `profiles/sandbox-claude.sb.tmpl` |
| `SessionChannel` proto | `packages/tddy-service/proto/sandbox.proto` |
| In-jail runner + relay + egress shim | `packages/tddy-tools/src/sandbox_runner.rs` |
| MCP allowlist + claude argv | `packages/tddy-tools/src/sandbox_claude_spawn.rs` |
| Daemon bridge | `packages/tddy-daemon/src/sandbox_session.rs` |
| Remote allowlist (shared) | `packages/tddy-workflow-recipes/src/permissions.rs` |
| Fake claude (e2e) | `packages/tddy-demo-tui` |

## Validation Results

**Last run:** 2026-06-27

```
tddy-sandbox-darwin                              ok
tddy-tools sandbox_runner_acceptance             4 passed
tddy-tools sandbox_runner_behavior_acceptance    2 passed
tddy-daemon sandbox_runner_spawn_smoke           1 passed
tddy-daemon sandbox_behavior_acceptance          5 passed
tddy-daemon sandboxed_claude_cli_acceptance      4 passed
tddy-daemon sandboxed_session_lifecycle          2 passed
cargo clippy (sandbox packages)                  ok
./test (full suite)                              see .verify-result.txt (includes tddy-demo-tui prebuild)
```
