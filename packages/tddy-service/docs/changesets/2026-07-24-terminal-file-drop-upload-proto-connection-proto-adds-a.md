# 2026-07-24 — terminal-file-drop-upload: `proto/connection.proto` adds a unary `UploadSessionFileChunk(UploadSessionFileChunkRequest{session_token,session_id,upload_id,file_name,data,last}) returns (UploadSessionFileChunkResponse{host_path})` on `ConnectionService` (the web drives chunking, so one unary RPC works over grpc-web and the LiveKit data channel; `host_path` is populated only on the final chunk). Regenerated Rust + `tddy-web/src/gen/connection_pb.ts`. Feature [web-terminal.md § File drop upload](../../../../docs/ft/web/web-terminal.md#file-drop-upload). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)

**Type:** Feature


