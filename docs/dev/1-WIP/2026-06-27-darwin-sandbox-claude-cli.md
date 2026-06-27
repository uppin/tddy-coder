# Changeset: Darwin-Sandboxed Claude Code CLI Sessions (Local gRPC)

**Date:** 2026-06-27  
**Packages:** `tddy-sandbox`, `tddy-sandbox-darwin`, `tddy-service`, `tddy-daemon`, `tddy-tools`, `tddy-core`, `tddy-testing-commons`  
**Feature PRD:** [docs/ft/1-WIP/PRD-2026-06-27-darwin-sandbox-claude-cli.md](../../ft/1-WIP/PRD-2026-06-27-darwin-sandbox-claude-cli.md)  
**Seatbelt spawn investigation (blocked):** [2026-06-27-darwin-sandbox-seatbelt-investigation.md](./2026-06-27-darwin-sandbox-seatbelt-investigation.md)

## Scope

- [x] **Package Documentation**: PRD + changeset created ✅
- [~] **Implementation**: Core sandbox stack landed; SessionChannel egress relay + seatbelt allowlist + MCP allowlist pending
- [~] **Testing**: Red tests revised for SessionChannel egress; awaiting green implementation
- [ ] **Integration**: Full daemon sandbox spawn blocked on seatbelt toolchain allowlist
- [ ] **Technical Debt**: Production readiness not verified; revert interim `egress_proxy.rs` WIP
- [ ] **Code Quality**: Linting / full `./test` not run for this changeset

## Architecture (Updated: 2026-06-27)

**Single egress path:** outbound network from the jail is **`(deny network*)`**. The
sandbox cannot dial out. The host daemon dials into the in-jail gRPC server; all
external reachability (MCP tools, LLM HTTP) is relayed on the **same host-poll
`SessionChannel`**.

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

| Direction | Frames (today) | Frames (egress — pending) |
|-----------|----------------|---------------------------|
| Host → sandbox | `SubscribeTerminal`, `HostPoll`, `SandboxInput`, `ExecuteToolResponse` | + `EgressResponse` |
| Sandbox → host | `SessionTerminalOutput`, `ExecuteToolRequest` | + `EgressRequest` |

Debug probes retained: unary `Echo`, bidi `EchoStream`.

**Superseded designs (do not implement):**
1. Split RPCs: `StreamSandboxTerminalOutput` + `SandboxToolExecChannel`
2. Host loopback TCP/HTTP proxy + `HTTPS_PROXY` + `egress_proxy_listen` manifest
   — incompatible with `(deny network*)`; remove/revert interim `egress_proxy.rs`

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] M1: `tddy-sandbox` trait + context dir + Unsupported error
- [x] M2: `tddy-sandbox-darwin` Seatbelt impl + SBPL template
- [x] M3: `sandbox.proto` `SessionChannel` + `StartSessionRequest.sandbox` flag + codegen
- [x] M4: `tddy-tools sandbox-runner` + `SandboxSessionRelay` (host-poll queue)
- [~] M5: daemon `start_sandboxed_claude_cli_session` + `dial_and_bridge` — code landed; daemon acceptance blocked on seatbelt exec allowlist
- [~] M6: `ResumeSession` / `DeleteSession` lifecycle — partial
- [x] M7: non-darwin Unsupported → `failed_precondition`
- [ ] M8: **`EgressRequest` / `EgressResponse` on SessionChannel** + in-jail HTTP shim + host HTTP relay in `dial_and_bridge` (Updated: 2026-06-27)
- [ ] M8b: Seatbelt **`(deny network*)`** in production profile (remove `(allow network*)` / proxy-port exception)
- [ ] M9: MCP allowlist in sandbox claude spawn
- [ ] M10: Revise red acceptance tests (remove HTTPS_PROXY / TCP proxy assertions)
- [ ] Acceptance tests (all green)
- [ ] Production readiness validation

## Acceptance tests

| Test file | Status | Notes |
|-----------|--------|-------|
| `packages/tddy-sandbox/tests/unsupported_on_non_darwin.rs` | ✅ 1/1 | |
| `packages/tddy-sandbox-darwin/tests/seatbelt_confinement_acceptance.rs` | ✅ 2/2 | |
| `packages/tddy-tools/tests/sandbox_runner_acceptance.rs` | ✅ 4/4 | SessionChannel PTY + tool exec |
| `packages/tddy-tools/tests/sandbox_runner_behavior_acceptance.rs` | 🔴 1/2 | demo-tui ✅; SessionChannel egress ❌ (missing shim) |
| `packages/tddy-daemon/tests/sandbox_behavior_acceptance.rs` | 🔴 0/5 | spawn + SessionChannel egress not implemented |
| `packages/tddy-daemon/tests/sandboxed_claude_cli_acceptance.rs` | ❌ blocked | seatbelt spawn + harness |
| `packages/tddy-daemon/tests/sandboxed_session_lifecycle_acceptance.rs` | ❌ blocked | same |

### Red phase (Updated: 2026-06-27)

- [x] Add `EgressRequest` / `EgressResponse` to `sandbox.proto` (API defined; codegen pending build)
- [x] `SandboxSessionChannelHost` test driver in `tddy-testing-commons`
- [x] Revise `write_egress_probe_claude_script` — `session_channel=` markers, `TDDY_EGRESS_SHIM`
- [x] Revise runner + daemon behavior acceptance tests
- [ ] Extend `SandboxSessionRelay` to queue egress frames on `HostPoll`
- [ ] In-jail HTTP shim (`TDDY_EGRESS_SHIM`) in sandbox-runner
- [ ] Host `dial_and_bridge`: relay `EgressRequest` → outbound HTTP → `EgressResponse`
- [ ] Spawn manifest: `egress_via: session_channel`, `network_policy: deny`
- [ ] Seatbelt `allow_read_paths` + `(deny network*)`
- [ ] **Remove/revert** interim `egress_proxy.rs` and `HTTPS_PROXY` WIP
- [ ] `SandboxSessionManager` stop `SandboxHandle` on delete

## Implementation evidence

| Deliverable | Location |
|-------------|----------|
| Sandbox spec + context dir | `packages/tddy-sandbox/src/` |
| Seatbelt spawn + profile template | `packages/tddy-sandbox-darwin/src/spawn.rs`, `profiles/sandbox-claude.sb.tmpl` |
| `SessionChannel` proto (PTY + tools) | `packages/tddy-service/proto/sandbox.proto` |
| In-jail runner + relay | `packages/tddy-tools/src/sandbox_runner.rs` |
| Daemon bridge | `packages/tddy-daemon/src/sandbox_session.rs` |
| ~~Host TCP proxy (superseded)~~ | ~~`packages/tddy-daemon/src/egress_proxy.rs`~~ — remove in next green pass |
| Fake claude (Cypress e2e) | `packages/tddy-demo-tui` |

## Validation Results

**Last run:** 2026-06-27 (partial; pre-requirements-change)

```
tddy-sandbox unsupported_on_non_darwin          1 passed
tddy-sandbox-darwin seatbelt_confinement        2 passed
tddy-tools sandbox_runner_acceptance            4 passed
tddy-tools sandbox_runner_behavior_acceptance   1 passed, 1 failed (HTTPS_PROXY — obsolete test)
tddy-daemon sandbox_behavior_acceptance         0 passed, 5 failed (spawn timeout)
```

**Next validation target:** revised red tests asserting `EgressRequest` relay, not TCP proxy.
