# Seatbelt sandbox troubleshooting (tddy-sandbox-darwin)

Use when `sandbox-runner` fails to reach its ready marker or `sandbox-exec` exits with abort trap 6 (134).

## Reproduce at profile level

```bash
sandbox-exec -f /path/to/rendered.sb /bin/echo hi
```

Read crash reports under `~/Library/Logs/DiagnosticReports/` and runner boot logs under the session `egress_dir`.

## Common blockers

| Symptom | Cause | Fix |
|---------|-------|-----|
| `/bin/echo` SIGABRT before `main()` | dyld reads root dir `/` for shared cache | `(literal "/")` in `file-read*` |
| `getaddrinfo("localhost")` fails | No resolver in clean-env jail | Bind/dial literal `127.0.0.1`; SBPL uses `localhost` keyword only |
| `tool ipc bind: Operation not permitted` | `(deny network*)` blocks AF_UNIX | Allow `(local/remote unix-socket)` |
| IPC bind fails under `/tmp` project | Symlink path mismatch (`/tmp` vs `/private/tmp`) | Canonicalize paths in profile + spawn manifest |
| `openpty: Operation not permitted` | PTY devices need read | Allow `/dev/ptmx`, `/dev/ttys*` reads |
| `spawn claude in pty: Operation not permitted` | Default deny on `process-fork` | `(allow process-fork)` |
| `tool ipc bind: path must be shorter than SUN_LEN` | Socket path > 104 bytes | `SandboxSpec::short_ipc_socket_path` |
| `claude` SIGTRAP / `EPERM` at startup (`Trace/BPT trap: 5`) | V8/Node `claude` reads OS caches, tz data, dyld state, user config outside the explicit allow-list | `(allow file-read*)` in `sandbox-claude.sb.tmpl` (the one rule that lets `claude --version` run; `dynamic-code-generation` alone is insufficient). Trades away read confinement — see Outbound egress below |
| `Failed to connect to api.anthropic.com: ECONNREFUSED` | Agent dials out directly; `(deny network*)` refuses it | Set `HTTPS_PROXY`/`HTTP_PROXY` to the in-jail egress shim so the agent routes through the CONNECT tunnel (see Outbound egress below) |

## Outbound egress (HTTPS_PROXY CONNECT tunnel)

The jail has `(deny network*)` and never dials out. The agent reaches the network through an
**in-jail CONNECT proxy**: `runner.rs` exports `HTTPS_PROXY`/`HTTP_PROXY` to the `claude` PTY pointing
at the loopback egress shim; `claude` issues `CONNECT api.anthropic.com:443`; the shim relays the raw
(still TLS-encrypted) bytes over `SessionChannel` tunnel frames (`TunnelOpen`/`TunnelData`/
`TunnelClose`). The **host** (`sandbox_session.rs::spawn_tunnel`) opens the real outbound socket and
pumps bytes both ways. TLS stays end-to-end — the host never sees plaintext or credentials.

- This is **not** the rejected "host HTTPS proxy" design (that had the jail dial *out* to a host
  proxy, breaking `(deny network*)`). Here the proxy is in-jail on loopback; the host is a TCP relay.
- The legacy unary `EgressRequest`/`EgressResponse` path is retained only for the `GET /probe` check.
- **Read confinement is intentionally relaxed** (`(allow file-read*)`, required for the Node agent);
  write confinement remains the security boundary. Narrowing reads is future tech debt.
- Acceptance: `sandbox_runner_tunnels_https_proxy_connect_via_session_channel`. The daemon
  `StartSession` egress path reuses the same helpers but lacks a daemon-specific acceptance test yet.

## macOS sandbox logs

```bash
log show --predicate 'sender == "Sandbox"' --last 5m
```

## See also

- Profile template: `profiles/sandbox-claude.sb.tmpl`
- Agent skill: [.agents/skills/darwin-sandbox/SKILL.md](../../../../.agents/skills/darwin-sandbox/SKILL.md)
- Daemon: [connection-service.md](../../tddy-daemon/docs/connection-service.md#sandboxed-claude-code-cli-sessions)
