# tddy-session-files

Session file I/O behind `session_files.SessionFilesService`: the workflow files a recipe wrote, the
agent context directory a managed session reads its guidance from, terminal file uploads, and the
staged attachments and host documents a session start draws on. Thirteen RPCs, all of them a read or
a write under a path the **serving** host resolves from the caller's session token.

## Quick Start

### Testing
```bash
cargo test -p tddy-session-files
```

## Architecture

Every method resolves its root from what the host persisted about the session — never from a path, an
OS user or an allow-list row the request supplied — so the ports the crate is constructed with are
resolvers rather than values. A host builds `SessionFilesServiceImpl` from `SessionFilesPorts` and
registers its entry; peer routing is the host's, because forwarding a call to the daemon that holds
the bytes needs a transport layer this crate deliberately cannot reach.

## Documentation

### Product Requirements (What)
- [agent-context-sync.md](../../docs/ft/daemon/agent-context-sync.md) — keeping a managed agent's
  context directory in line with its repository
- [session-attachments.md](../../docs/ft/coder/session-attachments.md) — attaching documents to a
  session
- [session-files-inspector.md](../../docs/ft/web/session-files-inspector.md) — the Files tab
- [web-terminal.md](../../docs/ft/web/web-terminal.md) — terminal file drop

### Technical Implementation (How)
- [session-files-service.md](./docs/session-files-service.md) — the thirteen methods, the ports, and
  where routing lives
- [agent-context-sync.md](./docs/agent-context-sync.md) — the allow-list gate, the framing, the syncer
- [host-documents-and-attachments.md](./docs/host-documents-and-attachments.md) — scopes, staging,
  uploads and the attachment store

## Related Packages
- [tddy-service](../tddy-service/docs/) — owns `session_files.proto` and `types.proto`
- [tddy-daemon](../tddy-daemon/docs/connection-service.md) — the host: its ports, its routing, and
  `StartSession`, which materialises what this crate serves
- [tddy-worktree-service](../tddy-worktree-service/docs/worktree-service.md) — the sibling reader,
  gated on git's listing rather than an allow-list
- [tddy-daemon-kernel](../tddy-daemon-kernel/docs/) — publishes `HOST_DOCUMENT_FRAME_BYTES`
