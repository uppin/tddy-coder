# Terminal file upload (drop / Attach) — implementation

Product spec: [web-terminal.md § File drop upload](../../../docs/ft/web/web-terminal.md#file-drop-upload).

| Module | Role |
|--------|------|
| `components/connection/TerminalFileDropZone.tsx` | Drag-over overlay + drop handler wrapping both terminals |
| `components/connection/TerminalUploadButton.tsx` | Mobile Attach button (native multi-file picker), same hook |
| `hooks/useSessionFileUpload.ts` | Orchestration: drop id, per-file chunk loop, progress, path insertion |
| `lib/fileUploadChunks.ts` | `chunkFile()` + `UPLOAD_CHUNK_SIZE` / `UPLOAD_REQUEST_ENVELOPE_HEADROOM` |
| `lib/shellQuote.ts` | `joinQuotedPaths()` — escaped, space-separated, one trailing space, no newline |
| `lib/randomId.ts` | `randomUuid()` for the per-drop `upload_id` (see below) |
| `rpc/uploadProgress.tsx` | Shared progress store rendered by `UploadProgressIndicator` in the footer |

The web drives chunking, so progress is known client-side and one **unary**
`ConnectionService.UploadSessionFileChunk` works over both grpc-web and the LiveKit data channel — no
client-streaming RPC is needed. The daemon appends chunks in arrival order and returns the absolute
host path on the final chunk only, so `uploadFiles` never types an empty path.

## Three invariants worth not breaking

**1. The drop id must not require a secure context.** `randomUuid()`, not `crypto.randomUUID()` —
tddy-web is normally served over plain http on a LAN address. See
[insecure-origin-constraints.md](insecure-origin-constraints.md).

**2. One chunk request must fit one LiveKit data packet.** `UPLOAD_CHUNK_SIZE` is **48 KiB** and
`UPLOAD_REQUEST_ENVELOPE_HEADROOM` (8 KiB) reserves room for the RPC envelope plus the request's
`session_token` / `session_id` / `upload_id` / `file_name` (~460 bytes measured; the constant is a
generous multiple). `fileUploadChunks.test.ts` pins `UPLOAD_CHUNK_SIZE + headroom <=
MAX_CHUNK_FRAME_BYTES`, importing that bound from `tddy-livekit-web` so the two cannot drift apart
silently.

Why it matters: a larger request is split into chunk frames by the transport, and losing one frame
leaves the daemon's reassembler permanently incomplete — the RPC is never answered and the upload
stalls mid-file with no error at all. This was a real production failure at the previous 256 KiB
size. See
[rpc-multi-transport.md § A lost chunk frame wedges the call](../../../docs/ft/coder/rpc-multi-transport.md#a-lost-chunk-frame-wedges-the-call--deadlines-are-the-only-escape).

**3. Every chunk carries a deadline.** `useDaemonUploadChunk` passes `{ timeoutMs }`
(`UPLOAD_CHUNK_TIMEOUT_MS` = 20 s, overridable per call). A stalled chunk then fails that one file
via the existing `progress.failFile` path — its path is not typed, an error strip shows, and the
remaining files still upload — instead of leaving the drop pending forever. No retry: the append-based
RPC is not idempotent (a retried chunk would append twice), so failing the file is the honest
outcome.

## Ordering, throughput, and feedback

Files are uploaded **sequentially** and each file's chunks are awaited one at a time, so the loop
applies its own backpressure and insertion order follows *drop* order rather than completion order.
The cost is that a drop is round-trip bound: a verified 2 MB drop over the LiveKit data channel took
~40 s across 41 serial round trips (~47 KB/s, individual chunks 0.23–4.97 s). Since the only feedback
is the 64-px aggregate bar in the Host Stats Footer, users read a large drop as a failure until the
path finally appears — both bug reports against this feature after the fixes above were exactly that.

Two tracked follow-ups, neither implemented: an explicit `offset` on `UploadSessionFileChunk`
(write-at instead of append) would make arrival order irrelevant and allow several chunks in flight,
turning the transfer bandwidth-bound; and progress feedback belongs on the terminal itself (overlay,
or a placeholder replaced by the path on completion), not only in the screen footer.

## Re-dropping an already-uploaded file (Files tab → terminal)

An uploaded file stays reusable via the Inspector **Files** tab (see
[session-files-inspector.md](../../../docs/ft/web/session-files-inspector.md)): dragging a row onto
the terminal carries the file's already-uploaded **host path** in the drag's `DataTransfer` under the
private MIME `application/x-tddy-host-path` (the constant in `lib/hostPathDrag.ts`). `handleDrop`
checks `dataTransfer.getData(HOST_PATH_MIME)` first and, when present, inserts the shell-quoted path
via `joinQuotedPaths([hostPath])` and **returns without uploading** — the bytes are already on the
host. An OS file drag (carrying `DataTransfer.files`) is unchanged and still uploads. The click/tap
route (Files-tab **Insert** button) reaches the focused terminal through the runtime registry's
`insertInput` hook instead of the drop zone.

## Reuse: start-session attachments (no web UI yet)

The pre-session staging upload for start-session attachments is deliberately the **same** shape as
this flow — `UploadStagedAttachmentChunk` is `UploadSessionFileChunk` with `session_id` replaced by
`daemon_instance_id` and `upload_id` by `staging_id`. So `chunkFile()` / `UPLOAD_CHUNK_SIZE` and the
per-file chunk loop are reusable as-is; only the request builder differs. The daemon side is live and
`StartSession` materializes attachments before the agent spawns; nothing in this package calls it yet
— see
[connection-service.md § Start-session attachments](../../tddy-daemon/docs/connection-service.md#start-session-attachments)
before wiring a picker, in particular the rule that a staged batch is usable only by a session
started on the host it was uploaded to, and that only the **final** chunk marks a batch complete (an
incomplete upload is refused at materialization, not silently truncated).

## Tests

Units `randomId.test.ts` (6), `fileUploadChunks.test.ts` (5), `shellQuote`/`uploadProgress`; Cypress
`TerminalFileDropUpload` (4), `TerminalFileUploadFailure` (2), `MobileTerminalUploadButton` (2),
`TerminalFileUploadProgressFooter` (2), `TerminalDropInsertHostPath` (2, internal host-path drag).
The transport deadline itself is pinned in `packages/tddy-livekit-web/src/transport.test.ts`.
