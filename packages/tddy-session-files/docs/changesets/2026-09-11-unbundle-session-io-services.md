# 2026-09-11 — The crate, and the thirteen methods it serves

`tddy-session-files` serves `session_files.SessionFilesService`: the workflow files a recipe wrote,
agent context sync, terminal-drop uploads, and the staged attachments and host documents a session
start draws on. Ten modules, 155 tests.

Every method resolves its root from the caller's session token and from what the serving host
persisted about the session — never from a path, an OS user or an allow-list row the request
supplied. That is why `SessionFilesPorts` carries resolvers rather than values, and why
`SessionContextScopes` is a port: a caller holding a valid token for one of its sessions can name
any other session of the same user, so if the request's `agent` field chose the allow-list row it
could also be served any checkout's `.claude/**`, `.cursor/**` and `.mcp.json`.

Errors are `tddy_rpc::Status` throughout, not a crate error enum — one conversion at the boundary is
one chance for a documented code (`FAILED_PRECONDITION` for an incomplete staged upload) to drift.

`daemon_instance_id` routing is deliberately **not** here; the daemon wraps this implementation and
routes eight of the thirteen. Putting the fork on the served trait rather than on the transport is
what makes an in-process caller obey the instance id it named, and it puts authentication before
route classification on the five staging methods.

`ContextSource` is declared in `context_sync` and re-exported from the crate root, because
`split_session` (in `tddy-daemon`) implements the decision procedure against it and two declarations
of one trait is how the split half and the co-located half stop being obliged to sync identically.
`HostDocumentScope` is the generated `types.HostDocumentScope`, re-exported rather than mirrored, so
the proto3 zero value a hand-written mirror silently drops stays representable and therefore
refusable. `contained_in_scope_root` and `validate_relative_path` are re-exported from
`host_documents` rather than declared at the root: the crate carried both spellings for a while and
the root pair was the lenient one.

Docs: [session-files-service.md](../session-files-service.md),
[agent-context-sync.md](../agent-context-sync.md),
[host-documents-and-attachments.md](../host-documents-and-attachments.md).
Full record: [../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md](../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md).
