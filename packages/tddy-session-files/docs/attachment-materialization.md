# Attachment materialization (`tddy_session_files`)

Turning a start's attachments into files in a session's directory, and the host documents they may refer
to. About 5.1k production lines in the crate.

| Module | Holds |
|---|---|
| `svc_materialize_staged_attachment.rs` | `AttachmentState<'a>` and the staged-file and host-document materializers |
| `session_attachment_materialization.rs` | `impl AttachmentState`: `prepare_session_attachments` and `materialize_session_attachments` |
| `attachment_progress` | the progress sink and reporter, `AttachmentMaterialization` and the cleanup |

`AttachmentState<'a>` is borrowed for one call and carries `config`, `tddy_data_dir`, `staging_base_dir`
and `peer_routing`; no hand-off to a task needs an owned form. The daemon host builds it
(`DaemonSessionHost::attachment_state()` in `tddy-session-lifecycle`). The module names the peer routing
and local-instance helpers of `tddy-daemon-livekit` (`PeerRoute`, `local_instance_id_for_config`), so this
crate depends on it.

A host document attached by reference is read through `ReadHostDocument` with the scope `types.proto` defines
once (`HostDocumentScope`), which `connection.proto` and `session_files.proto` both import.
