# `session_files.SessionFilesService` (tddy-session-files)

Thirteen RPCs over a session's files. The proto is
`packages/tddy-service/proto/session_files.proto`; the implementation is `src/service.rs`; each
family's policy and I/O lives in its own module beside it.

## The surface

| RPC | Shape | Module | What it does |
|---|---|---|---|
| `ListSessionWorkflowFiles` | unary | `session_workflow_files` | Allowlisted workflow basenames present under the session directory |
| `ReadSessionWorkflowFile` | unary | `session_workflow_files` | UTF-8 text of one allowlisted basename |
| `StreamContextManifest` | server stream | `context_files` | One entry per allow-listed agent-config path, with `sha256` and `size_bytes` |
| `StreamReadContextFile` | server stream | `context_files` | One allow-listed file's raw bytes |
| `StreamReadContextFileBatch` | server stream | `context_files` | Several allow-listed files in one call |
| `UploadSessionFileChunk` | unary | `session_file_upload` | Append one ordered chunk of a dropped file; the last returns its host path |
| `ListSessionUploads` | unary | `session_uploads` | Every uploaded file, flat and newest-first |
| `DeleteSessionUpload` | unary | `session_uploads` | Remove one uploaded file |
| `UploadStagedAttachmentChunk` | unary | `session_attachment_staging` | Append one chunk of a file staged before the session exists |
| `ListStagedAttachments` | unary | `session_attachment_staging` | A caller's staged batches, newest-first |
| `DeleteStagedAttachment` | unary | `session_attachment_staging` | Remove one staged file |
| `ReadHostDocument` | unary | `host_documents` | The bytes of a document that already exists on a connected host |
| `StreamReadHostDocument` | server stream | `host_documents` | The same resolution, for a document past the unary ceiling |

Two more modules serve the same subsystem without owning an RPC of their own:
`session_attachments` is the attachment store `StartSession` materialises into,
`session_context_docs` is the list of a session's planning documents and the reader for their
contents, and `stack_doc_attachments` builds the reference list a planned PR's child session is
spawned with. `context_sync` is the decision procedure that drives the three context reads.

## Nothing in a request names a root

Every method resolves its root from the caller's `session_token` and from what the serving host
persisted about the session. That is the crate's whole authorization model, and it is why
`SessionFilesPorts` carries resolvers rather than values.

| Port | The host's answer |
|---|---|
| `os_users` | Which OS user a session token belongs to. An unknown or expired token is `UNAUTHENTICATED`; a known identity with no OS-user mapping is `PERMISSION_DENIED`, because only an operator can fix the second |
| `tddy_data_dir` | This host's data directory — `projects/` under it answers the `PROJECT_REPO` scope, and a user's sessions base derives from it |
| `staging_base_dir` | The restart-cleared pre-session staging root |
| `max_attachment_bytes` | The cap a context read and a streamed host document are refused by **before** their first frame |
| `daemon_instance_id` | Stamped on every staged-attachment entry, so a `StagedAttachmentRef` names the host holding the bytes |
| `context_scopes` | Which checkout a session's agent guidance is read from, and which allow-list row it is served under |
| `context_read_deadline` | How long one blocking context read may take — the host's `spawn_worker_request_timeout`, which the refusal names so an operator who hits it knows what to raise |

`context_scopes` is a port, not a lookup, for a specific reason: resolving a session to its checkout
reads the sandbox registry as well as the session metadata. The consequence is the one that matters
for authorization — the **request's** `agent` field never decides the allow-list row. Authorization
here is per OS user, so a caller holding a valid token for one of its sessions can name any other
session of the same user; if the request chose the row it could also be served any checkout's
`.claude/**`, `.cursor/**` and `.mcp.json`, which are the files that routinely carry API tokens in
MCP `env` blocks.

## Errors are `Status`

Every method answers an RPC and every module speaks `tddy_rpc::Status` directly. There is no crate
error enum: it would buy one conversion at the boundary and one chance for a documented code —
`FAILED_PRECONDITION` for an incomplete staged upload, pinned in `types.proto` — to drift into
something else on the way out.

## Truncation is refused, never returned

A staged file whose upload never completed has no final chunk, and a fetch of it is refused rather
than answered with the bytes that did arrive. Handing back a prefix is worse than an error: the
caller cannot tell a short file from an interrupted one, and a session started from it materialises a
partial attachment that looks deliberate. The same rule governs the size caps — an over-cap document
is refused before the first frame, never cut short, because once frames have started a consumer
cannot tell a truncated document from a whole one.

## Frame sizes are a wire contract

`StreamReadHostDocument` and the context-file reads both frame at
`tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES` (48 KiB), so a consumer that switches between them
sees the same boundaries. That constant is published in the kernel, and it is the only thing this
subsystem reaches outside itself for. A compile-time assertion pins it plus 8 KiB of envelope
headroom at or below `tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES` — a context read or a host
document must never engage the transport's chunk codec, where one lost frame wedges a call
permanently with no error.

## Routing is the host's, not the crate's

`daemon_instance_id` routing is **not** implemented here. Forwarding a call to the peer that holds
the bytes needs the eligible-daemon roster, the common room slot and the per-method LiveKit
forwarding clients — all of which are a daemon's transport layer, and a session-file reader that
reached for them would be back inside the module this crate was extracted from.

So the daemon wraps this implementation. `PeerRoutedSessionFiles`
(`packages/tddy-daemon/src/connection_service/svc_session_files_ports.rs`) implements the same
generated trait, decides the route, and either forwards or delegates inward. **Eight of the thirteen
route**: the three context streams, the three staging methods and both host-document reads. The other
five — the two workflow-file methods and the three upload methods — address the session directory on
the host the caller is already talking to.

Routing sits on the trait rather than on the transport deliberately. Put it on the transport and an
in-process caller — the daemon's own `StartSession` materialising an attachment — is served locally
whatever `daemon_instance_id` it named. On the trait, one decision serves a request arriving on the
wire and a request the daemon makes for itself, so the two cannot disagree about which host holds a
file. It also fixes the order the five staging methods depend on: they authenticate **before** they
classify, which is what stops an unauthenticated request driving an outbound forward.

Every routing decision is made by a `ConnectionServiceImpl` method rather than re-derived in the
wrapper, for the same reason.

## The daemon still owns `StartSession`

`StartSession` and `StreamStartSession` stay on `connection.ConnectionService`, and they are what
materialises a `StagedAttachmentRef` or a `HostDocumentRef` into a session's
`artifacts/attachments/`. So `types.proto` holds `HostDocumentScope`: a session start names the
staged attachments to materialise and each carries a scope, so the enum is reached by two separately
served services. It is re-exported from this crate's root rather than mirrored in Rust, so a caller
never converts and `HostDocumentScope::Unspecified` — the proto3 zero value a hand-written mirror
silently drops — stays representable and therefore refusable.

## Tests

`cargo test -p tddy-session-files` is 155 tests. The suites that carry the cross-cutting proof:

| Suite | Where | What it pins |
|---|---|---|
| `context_files_acceptance.rs`, `context_file_frames_unit.rs` | this crate | the allow-list gate, the caps, byte-exactness, the batch, and the framing against the chunk budget |
| `context_read_deadline_acceptance.rs` | this crate | a stalled read is refused with `DEADLINE_EXCEEDED` rather than hanging the RPC |
| `staged_attachment_path_validation.rs` | this crate | the basename and containment guards |
| `pr_stack_child_doc_attachment_acceptance.rs`, `pr_stack_context_docs_acceptance.rs` | this crate | the documents a planned PR's child is spawned with |
| `session_files_service_acceptance.rs` | `tddy-daemon` | the registered coordinate serves, and a request naming another daemon forwards rather than being served locally |
| `context_sync_acceptance.rs`, `staging_rpc_acceptance.rs`, `session_workflow_files_rpc.rs`, `session_file_upload_rpc.rs`, `session_uploads_rpc.rs`, `staging_forwarding_acceptance.rs`, `session_attach_staging_scope_acceptance.rs`, `session_attach_cross_host_acceptance.rs`, `session_room_acceptance.rs` | `tddy-daemon` | the behaviours that need a daemon — the split-context directory builder, the routing fork, the test service and its token |

The cross-host suites are the ones to be careful with. They start two daemons against one LiveKit
container and are genuinely load-sensitive, so run them individually and re-run a failure in
isolation before believing it. They are also the suites where a green run can prove nothing: a
forwarding assertion has to be made against the **peer's** state, because a test that stages onto
what it believes is the peer and reads back from the same host asserts success without a byte
crossing.

## See also

- [agent-context-sync.md](./agent-context-sync.md) — families J in detail
- [host-documents-and-attachments.md](./host-documents-and-attachments.md) — families R and S in detail
- [connection-service.md](../../tddy-daemon/docs/connection-service.md) — the host, and `StartSession`
