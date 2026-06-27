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

## macOS sandbox logs

```bash
log show --predicate 'sender == "Sandbox"' --last 5m
```

## See also

- Profile template: `profiles/sandbox-claude.sb.tmpl`
- Agent skill: [.agents/skills/darwin-sandbox/SKILL.md](../../../../.agents/skills/darwin-sandbox/SKILL.md)
- Daemon: [connection-service.md](../../tddy-daemon/docs/connection-service.md#sandboxed-claude-code-cli-sessions)
