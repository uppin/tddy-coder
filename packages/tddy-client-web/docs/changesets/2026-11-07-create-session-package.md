# 2026-11-07 — Upload, failure-text and file-listing helpers lowered from `tddy-web` (#web-split 7/17, PR #844)

- `rpcFailure` (`rpcFailureText`, `hostNotConnected`), `randomId` (`randomUuid`), `fileUploadChunks`
  (`UPLOAD_CHUNK_TIMEOUT_MS`) and `worktreeFilesApi`, with their tests, moved here from `tddy-web`: both
  `tddy-session-create-web` and `tddy-web` use them.
