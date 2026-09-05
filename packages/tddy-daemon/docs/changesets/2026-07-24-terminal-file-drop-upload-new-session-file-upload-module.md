# 2026-07-24 — terminal-file-drop-upload: new `session_file_upload` module

**Type:** Feature

`write_upload_chunk` basename-validates both `upload_id` and `file_name`, appends ordered chunks to `{session_dir}/uploads/<upload_id>/<file_name>`, and applies a canonicalize-and-contain guard rooted at the trusted `{session_dir}/uploads` root (traversal/separator segment → `InvalidArgument`), returning the absolute host path on the final chunk. `ConnectionService::upload_session_file_chunk` authenticates the session token and validates the session-id segment before any filesystem access; delegated through the tonic adapter. Tests: `session_file_upload_rpc` 9. Feature [web-terminal.md § File drop upload](../../../../docs/ft/web/web-terminal.md#file-drop-upload). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon)
