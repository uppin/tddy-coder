# Changeset: terminal-file-drop-upload — drag a file onto Ghostty → upload to the session dir → type its host path

**Date:** 2026-07-24
**Branch:** `feat-drag-file-on-ghostty`
**Packages:** `tddy-service`, `tddy-daemon`, `tddy-web`
**Feature PRD:** [docs/ft/web/web-terminal.md § File drop upload](../../ft/web/web-terminal.md#file-drop-upload) · [host-stats-footer.md § Upload progress](../../ft/web/host-stats-footer.md#upload-progress-drag-to-upload)

## Summary

Dragging one or more files from the host OS onto the web terminal viewport (either transport —
gRPC `GhosttyTerminalGrpc` or LiveKit `GhosttyTerminalLiveKit`) uploads each file to
`{session_dir}/uploads/{drop_id}/{filename}` on the host and then **types the uploaded files'
absolute host paths into the terminal input** (space-separated, each shell-escaped, one trailing
space, no newline) — emulating a native terminal file-drag. Aggregate upload progress renders in
the screen-level **Host Stats Footer** and auto-hides on completion; a failed file is skipped
(its path is not typed) and surfaced as a transient footer error. On mobile (no OS drag-drop) the
same flow is initiated from an **Attach** button in the **Keyboard strip**, which opens the native
multi-file picker.

The web drives chunking, so upload progress is known client-side and one **unary**
`UploadSessionFileChunk` RPC works over both grpc-web and the LiveKit data channel — no
client-streaming RPC is needed. The daemon appends ordered chunks under a per-drop uploads folder
with a canonicalize-and-contain guard and returns each file's absolute host path on its last chunk.

## Design decisions (from planning interview)

- **Inserted path:** absolute host path, shell-escaped (single-quote wrapped, `'\''` for embedded
  quotes), trailing space, **no** auto-Enter.
- **Transports:** both gRPC and LiveKit in this changeset.
- **Multiple files:** supported; concurrent under one drop id, inserted as one space-separated run.
- **Destination + collisions:** `uploads/{drop_id}/{filename}` — a fresh UUID subfolder per drop,
  so original filenames are preserved and collisions are impossible.
- **Progress UI:** one aggregate determinate bar `"{n} files · {pct}%"` in the footer, auto-hide;
  transient error on per-file failure.
- **Limits/errors:** no client-side size cap; mid-upload failures are surfaced and the file is
  skipped (no path inserted), remaining files proceed.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] `tddy-service`: add `UploadSessionFileChunk` RPC + request/response messages to
      `proto/connection.proto`; regenerate Rust + TS bindings
- [x] `tddy-daemon`: `session_file_upload` module — `write_upload_chunk(sessions_base, session_id,
      upload_id, file_name, data, last) -> host_path`, basename sanitization + canonicalize-contain
      guard rooted at `{session_dir}/uploads/{upload_id}/`, append semantics
- [x] `tddy-daemon`: wire `upload_session_file_chunk` into `ConnectionService`
      (`connection_service.rs` + `connection_tonic_adapter.rs`), unauthenticated-token rejection
- [x] `tddy-web`: `src/lib/shellQuote.ts` — `shellQuotePath(path)`
- [x] `tddy-web`: `src/lib/fileUploadChunks.ts` — `chunkFile(file, size)` + `UPLOAD_CHUNK_SIZE`
- [x] `tddy-web`: `src/rpc/uploadProgress.tsx` — `UploadProgressProvider`,
      `useUploadProgressController()`, `useUploadProgressSnapshot()`
- [x] `tddy-web`: `src/hooks/useSessionFileUpload.ts` — orchestrates chunked upload + progress +
      escaped-path insertion via an injected `insertInput`/`uploadChunk`
- [x] `tddy-web`: `src/components/connection/TerminalFileDropZone.tsx` — dragover overlay + drop
- [x] `tddy-web`: `src/components/connection/TerminalUploadButton.tsx` — mobile Attach button
- [x] `tddy-web`: `src/components/sessions/UploadProgressIndicator.tsx` — aggregate bar + error
- [x] `tddy-web`: wire drop zone + upload button into `GhosttyTerminalGrpc` and
      `GhosttyTerminalLiveKit`; thread `insertInput` (`sendInput` / `enqueueTerminalInput`)
- [x] `tddy-web`: mount `UploadProgressIndicator` in `HostStatsFooter`; wrap `SessionsDrawerScreen`
      in `UploadProgressProvider`
- [x] `tddy-web`: enable the mobile affordance slot on the LiveKit path
      (`SessionLiveKitTerminal` → `showMobileKeyboard`)
- [x] `tddy-web`: add `TEST_IDS` entries (`terminalDropOverlay`, `terminalUploadButton`,
      `uploadProgressIndicator`, `uploadProgressError`) + page objects

## Acceptance tests

- [x] `packages/tddy-web/cypress/component/TerminalFileDropUpload.cy.tsx`
- [x] `packages/tddy-web/cypress/component/TerminalFileUploadProgressFooter.cy.tsx`
- [x] `packages/tddy-web/cypress/component/MobileTerminalUploadButton.cy.tsx`
- [x] `packages/tddy-web/cypress/component/TerminalFileUploadFailure.cy.tsx`

## Unit / integration tests

- [x] `packages/tddy-daemon/tests/session_file_upload_rpc.rs` (Rust integration — daemon handler)
- [x] `packages/tddy-web/src/lib/shellQuote.test.ts`
- [x] `packages/tddy-web/src/lib/fileUploadChunks.test.ts`
- [x] `packages/tddy-web/src/rpc/uploadProgress.test.ts`

## Validation Results

### PR-wrap validation (2026-07-24)

Four review passes (validate-changes, validate-tests, validate-prod-ready, analyze-clean-code) plus lint/test:

**Critical (1) — fixed:**
- `session_file_upload.rs`: `upload_id` was an unvalidated path segment and the containment guard was
  rooted at a directory derived from that untrusted input (a tautology), allowing an authenticated
  client to traverse and append to arbitrary files. **Fix:** both `upload_id` and `file_name` are now
  validated as basenames, and the canonicalize-and-contain guard is rooted at the trusted
  `{session_dir}/uploads` root with a `starts_with` check. Locked by new tests
  `rejects_a_parent_traversal_upload_id` and `rejects_an_upload_id_containing_a_path_separator`.

**Warning (1) — fixed:**
- `uploadProgress.tsx`: the auto-hide `setTimeout` was untracked — a second failed drop could clear the
  newer error early, and the timer leaked on unmount. **Fix:** the timer is now owned by the store
  (tracked handle, cancelled on `startDrop`/`clearError`), `unref`'d so it never holds a test process,
  and cleared via `UploadProgressProvider`'s unmount effect (`store.dispose()`).

**Clean-code / prod-ready (fixed):**
- Hoisted the terminal element out of the inline IIFE in `GhosttyTerminalLiveKit.tsx` (matches the Grpc
  sibling) and narrowed `sessionToken`/`sessionId` once into `uploadTarget`, removing four non-null
  assertions.
- Extracted the repeated Rust error string to `UNSAFE_SEGMENT_ERR`.
- Removed the unused `inputRef` in `TerminalUploadButton.tsx`.
- Defensive: `useSessionFileUpload` no longer inserts an empty host path.

**Test quality (strengthened):**
- Pinned the gated progress percent to exactly `0` (`TerminalFileUploadProgressFooter.cy.tsx`).
- The mobile spec now reconstructs and asserts the uploaded bytes exactly (`MobileTerminalUploadButton.cy.tsx`).
- Added a `dragOverWith` helper and a justifying comment for the runtime-UUID loose matcher.

**Final gate:** `cargo test -p tddy-daemon --test session_file_upload_rpc` → 9 passed; `cargo clippy -p
tddy-daemon --all-targets -- -D warnings` → clean; `cargo fmt` → clean; `bun test` (shellQuote,
fileUploadChunks, uploadProgress) → 18 pass; Cypress component (4 upload specs + GhosttyTerminalLiveKit
regression) → 25/25 pass.

## Delta summary

### `tddy-service`

`proto/connection.proto`:

```proto
service ConnectionService {
  // ...
  rpc UploadSessionFileChunk(UploadSessionFileChunkRequest) returns (UploadSessionFileChunkResponse);
}

message UploadSessionFileChunkRequest {
  string session_token = 1;
  string session_id = 2;
  // Per-drop UUID grouping all files of one drag gesture into uploads/<upload_id>/.
  string upload_id = 3;
  // Basename only — path separators, "." / ".." and empty are rejected by the daemon.
  string file_name = 4;
  // Next chunk of this file's bytes; chunks for a given (upload_id, file_name) arrive in order.
  bytes data = 5;
  // True on the final chunk of this file — response then carries the absolute host_path.
  bool last = 6;
}

message UploadSessionFileChunkResponse {
  // Absolute host path of the written file; populated only when the request's `last` was true.
  string host_path = 1;
}
```

Regenerate Rust (`prost`) and TS (`@bufbuild/protobuf`) bindings.

### `tddy-daemon`

**New file** `src/session_file_upload.rs`:

- `pub fn upload_dir_for(sessions_base: &Path, session_id: &str, upload_id: &str) -> PathBuf` —
  `unified_session_dir_path(sessions_base, session_id).join("uploads").join(upload_id)`.
- `pub fn write_upload_chunk(sessions_base, session_id, upload_id, file_name, data, last) ->
  Result<Option<PathBuf>>` — validates `file_name` is a pure basename (reject empty, `.`, `..`,
  `/`, `\`, and any name whose `Path::file_name()` differs from the input); `create_dir_all` the
  per-drop dir on first chunk; open the target with append + create; write `data`; **canonicalize
  the parent and assert it is contained in the canonicalized uploads dir** before writing (guard
  against symlink/`..` escape); on `last`, return `Some(absolute_path)`, else `None`.

**Wire into `ConnectionService`** (`connection_service.rs`, adapter `connection_tonic_adapter.rs`):
`async fn upload_session_file_chunk(req)` — reject invalid `session_token` with `unauthenticated`
(parity with existing methods), resolve `sessions_base = self.tddy_data_dir`, call
`write_upload_chunk`, map the returned path to `UploadSessionFileChunkResponse { host_path }`.

### `tddy-web`

**New files:**

- `src/lib/shellQuote.ts` — `shellQuotePath(path: string): string` (POSIX single-quote quoting:
  wrap in `'…'`, replace embedded `'` with `'\''`); `joinQuotedPaths(paths: string[]): string`
  returning the space-separated run with a trailing space.
- `src/lib/fileUploadChunks.ts` — `UPLOAD_CHUNK_SIZE` (256 KiB) and
  `chunkFile(file: File, size?): Blob[]` yielding ordered slices covering all bytes (a 0-byte file
  yields exactly one empty final chunk so `last` still fires and a host path returns).
- `src/rpc/uploadProgress.tsx` — `UploadProgressProvider`, `useUploadProgressController()`
  (`startDrop(fileCount, totalBytes)`, `advance(bytes)`, `failFile(name)`, `finishDrop()`), and
  `useUploadProgressSnapshot(): { active, fileCount, percent, error }` for the footer. Subscribe/
  notify store shared across the two subtrees; `finishDrop()` schedules the auto-hide.
- `src/hooks/useSessionFileUpload.ts` — `useSessionFileUpload({ uploadChunk, insertInput })`
  returning `uploadFiles(files: File[]): Promise<void>`. Generates one `upload_id` per call; for
  each file chunks via `chunkFile`, calls `uploadChunk({ uploadId, fileName, data, last })` in
  order, `advance()`-ing progress; on success collects `host_path`; on error `failFile()` and
  skips; after all files, `insertInput(joinQuotedPaths(successfulPaths))` (only if ≥1 succeeded),
  then `finishDrop()`.
- `src/components/connection/TerminalFileDropZone.tsx` — wraps the terminal region; `onDragOver`
  shows the overlay (`terminal-drop-overlay`), `onDrop` reads `dataTransfer.files` → `uploadFiles`.
- `src/components/connection/TerminalUploadButton.tsx` — `terminal-upload-button` label with a
  hidden `<input type="file" multiple>`; `onChange` → `uploadFiles`.
- `src/components/sessions/UploadProgressIndicator.tsx` — reads the snapshot; renders the aggregate
  bar (`upload-progress-indicator`, attrs `data-upload-percent`, `data-upload-file-count`) and
  transient `upload-progress-error`; renders nothing when `!active && !error`.

**Modified:**

- `GhosttyTerminalGrpc.tsx` / `GhosttyTerminalLiveKit.tsx` — wrap the canvas in
  `TerminalFileDropZone`, place `TerminalUploadButton` in the mobile Keyboard strip, thread
  `insertInput` = `sendInput` (gRPC) / `enqueueTerminalInput` (LiveKit) and `uploadChunk` (a
  closure over the `ConnectionService` client).
- `SessionLiveKitTerminal.tsx` — set `showMobileKeyboard` so the LiveKit path renders the strip.
- `HostStatsFooter.tsx` — mount `UploadProgressIndicator`.
- `SessionsDrawerScreen.tsx` — wrap in `UploadProgressProvider`.
- `cypress/support/testIds.ts` — new ids + page objects (`terminalFileUploadPage`,
  `uploadProgressFooterPage`).

## Out of scope

- Uploading directories / folder drops (only files).
- Resumable / retryable uploads (a failed file is skipped, not retried).
- A client-side size cap or type filtering.
- Uploading into the worktree checkout rather than the session dir.
