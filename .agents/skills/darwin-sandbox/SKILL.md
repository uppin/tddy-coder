---
name: darwin-sandbox
description: Guide for macOS Seatbelt sandboxed Claude CLI sessions in tddy-daemon — spawn, SessionChannel egress, SBPL profile tuning, and debugging sandbox-exec failures. Use when working on tddy-sandbox, tddy-sandbox-darwin, sandbox-runner, sandbox_session, seatbelt profiles, or diagnosing in-jail spawn/egress/tool-IPC issues.
---

# Darwin Sandbox (Claude CLI, Local gRPC)

Local sibling of [remote-codebase mode](../../../docs/ft/daemon/remote-codebase-mode.md): the agent runs **inside** a macOS Seatbelt jail; the host daemon owns the git worktree and serves tools over a host-initiated gRPC `SessionChannel`.

## Architecture (read first)

```
Client → tddy-daemon (ConnectionService, sandbox=true)
           │ spawn sandbox-exec + dial SessionChannel
           ▼
         tddy-tools sandbox-runner (in jail, no outbound network)
           ├─ claude PTY → TerminalOutput frames
           ├─ MCP → ExecuteToolRequest → host tool_engine
           └─ HTTPS_PROXY CONNECT proxy → Tunnel{Open,Data,Close} → host TCP relay
```

**Key invariant:** the sandbox never dials out. `(deny network*)` in production SBPL. All external reachability is relayed on `SessionChannel`.

**Egress (HTTPS_PROXY CONNECT tunnel):** `runner.rs` sets `HTTPS_PROXY`/`HTTP_PROXY` for the agent to the in-jail loopback egress shim. claude issues `CONNECT api.anthropic.com:443`; the shim allocates a tunnel and relays raw (still TLS-encrypted) bytes over `SessionChannel` `TunnelOpen`/`TunnelData`/`TunnelClose` frames. The **host** (`sandbox_session.rs::spawn_tunnel`) opens the real outbound socket and pumps bytes both ways — TLS stays end-to-end, so the host never sees plaintext or credentials. The legacy unary `EgressRequest`/`EgressResponse` path remains only for the `GET /probe` connectivity check. Acceptance: `sandbox_runner_tunnels_https_proxy_connect_via_session_channel`.

## Code map

| Concern | Location |
|---------|----------|
| `Sandbox` trait + context dir | `packages/tddy-sandbox/src/` |
| Seatbelt spawn + SBPL template | `packages/tddy-sandbox-darwin/src/spawn.rs`, `profiles/sandbox-claude.sb.tmpl` |
| Proto (`SessionChannel`, egress) | `packages/tddy-service/proto/sandbox.proto` |
| In-jail runner + relay | `packages/tddy-tools/src/sandbox_runner.rs` |
| MCP allowlist for claude spawn | `packages/tddy-tools/src/sandbox_claude_spawn.rs` |
| Daemon bridge + lifecycle | `packages/tddy-daemon/src/sandbox_session.rs` |
| StartSession routing | `packages/tddy-daemon/src/connection_service.rs` (`start_sandboxed_claude_cli_session`) |

## Debugging spawn failures

When `sandbox-runner` never reaches the ready marker, **do not assume network policy**. Follow the investigation playbook:

**Doc:** [packages/tddy-sandbox-darwin/docs/troubleshooting.md](../../../packages/tddy-sandbox-darwin/docs/troubleshooting.md)

**Repro at profile level (not test level):**

```bash
sandbox-exec -f /path/to/rendered.sb /bin/echo hi
```

**Common fixes (historical blockers):**

1. `(literal "/")` in `file-read*` — dyld shared-cache lookup
2. Bind/dial `127.0.0.1`, not `localhost` (no resolver in clean env)
3. SBPL TCP rules use `localhost` keyword; runtime uses literal IP
4. `(local/remote unix-socket)` for tool-IPC AF_UNIX bind
5. **Canonicalize** paths — `/tmp` vs `/private/tmp` symlink mismatch
6. PTY device reads on `/dev/ptmx`, `/dev/ttys*`
7. `(allow process-fork)` for PTY child
8. Short out-of-tree IPC socket path (`SUN_LEN` limit)

**Logs:** runner boot markers in stderr; macOS `log show --predicate 'sender == "Sandbox"'`.

## Acceptance tests (green = feature works)

```bash
./dev cargo test -p tddy-sandbox-darwin
./dev cargo test -p tddy-tools --test sandbox_runner_acceptance --test sandbox_runner_behavior_acceptance
./dev cargo test -p tddy-daemon --test sandbox_behavior_acceptance --test sandboxed_claude_cli_acceptance --test sandboxed_session_lifecycle_acceptance
```

## When changing SBPL or spawn manifest

- Run seatbelt confinement test: `packages/tddy-sandbox-darwin/tests/seatbelt_confinement_acceptance.rs`
- Re-run full sandbox daemon acceptance suite (above)
- Update [troubleshooting.md](../../../packages/tddy-sandbox-darwin/docs/troubleshooting.md) if you discover a new blocker pattern
- **Never** add fallbacks for unsupported platforms — map to `failed_precondition`

## Superseded designs (do not reintroduce)

- Split RPCs: `StreamSandboxTerminalOutput` + `SandboxToolExecChannel` (replaced by `SessionChannel`)
- **Host-side** loopback TCP proxy + `egress_proxy.rs` (jail dialing *out* to a host proxy — broke `(deny network*)`). NOTE: this is distinct from the current **in-jail** `HTTPS_PROXY` CONNECT proxy (proxy runs *inside* the jail on loopback; host is a TCP relay over `SessionChannel`) — that IS the chosen egress design, see Architecture above.

## Related docs

- Feature: [claude-cli-session.md](../../../docs/ft/daemon/claude-cli-session.md)
- Feature: [remote-codebase-mode.md](../../../docs/ft/daemon/remote-codebase-mode.md)
- Technical: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md#sandboxed-claude-code-cli-sessions)
- Architecture: [tddy-sandbox architecture](../../../packages/tddy-sandbox/docs/architecture.md)
