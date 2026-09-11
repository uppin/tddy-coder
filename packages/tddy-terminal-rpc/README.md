# tddy-terminal-rpc

The terminal surface: the `terminal_session.TerminalSessionService` proto, its server, and the
transport-agnostic bridge that turns a live PTY into an RPC stream. Every terminal any tddy process
serves — a daemon's claude-cli session, a jailed session, a coder's bash tab — is answered from here.

## Quick Start

### Testing
```bash
cargo test -p tddy-terminal-rpc
```

## Architecture

One crate owns the proto, the service implementation and the streaming bridge, and hosts supply only
what differs between them through four traits: which terminal a `(session_id, terminal_id)` names,
who holds the control lease, how a login shell becomes a task, and which identity a session token
belongs to. A host registers `build_terminal_session_entry` over any `tddy-rpc` transport (LiveKit,
stdio, Connect-HTTP) and the generated `TerminalSessionServiceTonicAdapter` over gRPC, so the replay,
offset and acknowledgement semantics are one implementation whichever coordinate a client reaches.

## Documentation

### Product Requirements (What)
- [terminal-sessions.md](../../docs/ft/daemon/terminal-sessions.md) — multiple terminals per session,
  the control mutex, lazy replay
- [terminal-replay-lazy-scroll.md](../../docs/ft/web/terminal-replay-lazy-scroll.md) — the scroll-up
  history flow the replay model serves
- [web-terminal.md](../../docs/ft/web/web-terminal.md) — the browser terminal

### Technical Implementation (How)
- [terminal-session-service.md](./docs/terminal-session-service.md) — the nine methods, the ports, the
  bridge, and the replay contract

## Related Packages
- [tddy-pty](../tddy-pty/) — the PTY primitives a host's terminals are built from
- [tddy-task](../tddy-task/docs/terminal-capture.md) — `TerminalCapture`, the replay ring the bridge reads
- [tddy-daemon](../tddy-daemon/docs/connection-service.md) — one host: claude-cli and sandboxed sessions
- [tddy-coder](../tddy-coder/README.md) — the other host: a session's own participant
