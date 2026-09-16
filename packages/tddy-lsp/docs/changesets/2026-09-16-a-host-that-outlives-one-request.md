# 2026-09-16 — What a host that outlives one request needs

**Type:** Fix

Four defects in the registry, all harmless in a process that handles one request and structural in
one that does not. `tddy-daemon` and `tddy-sandbox-app` have been running this registry with all
four.

- **Document versions belong to the client.** `LspClient` now owns per-URI version state and an
  open-document set, with `did_change` and `did_close` beside `did_open`. The only working
  `did_change` previously lived in `tddy-code-restructuring`'s backend with a per-process counter, so
  a second caller began again at version 1 against a server that had seen 40 — a violation the server
  may ignore, surfacing as an edit that silently did nothing. A notification naming a URI the client
  never opened is refused (`LspError::DocumentNotOpen`) rather than dropped.
- **Using a borrowed client counts as activity.** The idle tracker was refreshed only by
  `get_or_spawn`, so a caller working for minutes through `service.client` could have its server
  reaped mid-operation. The client carries an activity hook the registry installs after the
  handshake — after, so a spawn cannot backdate the idle clock.
- **Concurrent cold requests spawn one server.** `get_or_spawn` released its map lock before
  spawning, so two callers racing on a cold key each started a rust-analyzer and the second insert
  orphaned the first — a multi-gigabyte process, unreachable for reaping. A per-key spawn gate with a
  double-checked lookup replaces it; different keys still start concurrently.
- **The server's stderr is drained.** It was piped and never read, so past a pipe buffer the server
  blocked mid-write and stopped answering every request thereafter.

`did_open` on an already-open URI still restarts that document at version 1, because `bind_target`
re-opens every source on every call and `tddy-lsp-executor` depends on that; see the backlog entry of
that name.
