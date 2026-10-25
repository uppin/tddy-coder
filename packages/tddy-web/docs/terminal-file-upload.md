# Terminal file upload (drop / Attach) — implementation

Product spec: [web-terminal.md § File drop upload](../../../docs/ft/web/web-terminal.md#file-drop-upload).

| Module | Role |
|--------|------|
| `components/connection/TerminalFileDropZone.tsx` | Drag-over overlay + drop handler wrapping both terminals; registers for native drops in Tddy Desktop |
| `components/connection/nativeFileDrop.tsx` | `NativeFileDropProvider` / `useNativeFileDropTarget` (Tddy Desktop's window drops), `droppedFileUploadSource` |
| `components/connection/TerminalUploadButton.tsx` | Mobile Attach button (native multi-file picker), same hook |
| `hooks/useSessionFileUpload.ts` | Orchestration: drop id, per-file chunk loop, progress, path insertion; `UploadSource`, `fileUploadSource` |
| `lib/fileUploadChunks.ts` | `chunkFile()` + `UPLOAD_CHUNK_SIZE` / `UPLOAD_REQUEST_ENVELOPE_HEADROOM` |
| `lib/shellQuote.ts` | `joinQuotedPaths()` — escaped, space-separated, one trailing space, no newline |
| `lib/randomId.ts` | `randomUuid()` for the per-drop `upload_id` (see below) |
| `rpc/uploadProgress.tsx` | Shared progress store rendered by `UploadProgressIndicator` in the footer |
| `lib/sessionReadsLocalPaths.ts` | Whether a native drop hands a session real paths instead of uploading (see below) |

The web drives chunking, so progress is known client-side and one **unary**
`session_files.SessionFilesService.UploadSessionFileChunk` works over both grpc-web and the LiveKit
data channel — no client-streaming RPC is needed. The daemon appends chunks in arrival order and
returns the absolute host path on the final chunk only, so an upload never types an empty path.

**What is uploaded is an `UploadSource`** — a name, a size, a chunk count and `readChunk(i)` — so one
loop (`uploadSources`) serves a browser `File` (`fileUploadSource`, sliced by `chunkFile`) and a file
read off this machine through Tddy Desktop's window (`droppedFileUploadSource`). `uploadFiles` is
`uploadSources` over `fileUploadSource`. A source always has at least one chunk, so an empty file still
sends a final chunk and gets its path back.

**Paths reach the PTY as a paste.** `GhosttyTerminalSession` hands the drop zone `pasteInput`, which
calls the terminal handle's `paste` (ghostty-web's `Terminal.paste`): wrapped in `ESC [ 200 ~` …
`ESC [ 201 ~` when the program enabled bracketed paste (DEC 2004), plain otherwise, and forwarded
through `onData` to the feed like any keystroke. Claude Code enables bracketed paste and attaches a
pasted image path as an image.

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

Two tracked follow-ups, neither implemented: an explicit `offset` on
`SessionFilesService.UploadSessionFileChunk` (write-at instead of append) would make arrival order
irrelevant and allow several chunks in flight, turning the transfer bandwidth-bound; and progress
feedback belongs on the terminal itself (overlay, or a placeholder replaced by the path on
completion), not only in the screen footer.

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

## Tddy Desktop: drops arrive from the window, not the DOM

The desktop window keeps Tauri's native drag-drop handler, the only source of a dropped file's real
path, and with it on the page receives **no** HTML5 `dragover` / `drop` — the zone's own handlers
above never fire there. The window relays every drag step instead (`tddy-tauri-web`'s
`NativeFileDropBridge`; positions already in CSS pixels, see
[`tddy-desktop` file-drop.md](../../tddy-desktop/docs/file-drop.md)), and:

- **`NativeFileDropProvider`** (mounted in `index.tsx` only when `daemonTransportFlavour` is
  `webview-ipc`; `null` bridge in a browser) hit-tests each point with `document.elementFromPoint`,
  walking up to the innermost registered element. Hidden and `pointer-events: none` panes are never
  hit. It tells that target it is hovered and the others they are not, delivers the paths on `drop`,
  and ignores a drag that carries no paths (one started inside the page).
- **`TerminalFileDropZone`** registers its wrapper through `useNativeFileDropTarget`, shows the same
  `terminal-drop-overlay` while hovered, and on drop:
  - `readsLocalPaths` → `insertInput(joinQuotedPaths(paths))`, nothing read or uploaded;
  - otherwise → `bridge.describe(paths)`; folders are refused in `terminal-drop-refused` (shown for
    `DROP_REFUSAL_VISIBLE_MS`, 8 s), files go through `uploadSources` as `droppedFileUploadSource`s,
    each chunk a `bridge.read(path, i × UPLOAD_CHUNK_SIZE, UPLOAD_CHUNK_SIZE)`. A failed `describe`
    is refused the same way.
- **`readsLocalPaths`** is `sessionReadsLocalPaths(window, entry, servingInstanceId)`, computed in
  `SessionRuntime` and passed to the agent terminal and every bash terminal: inside the host
  application, the session on the daemon that application hosts, and `SessionEntry.readsHostPaths`
  set by that daemon (its agent is not jailed, managed-codebase, split from its checkout, on SSH, or a
  bare workspace). Everything else uploads, as a browser drop does.

## Shared with start-session attachments

The pre-session staging upload for start-session attachments is deliberately the **same** shape as
this flow — `UploadStagedAttachmentChunk` is the same service's `UploadSessionFileChunk` with
`session_id` replaced by `daemon_instance_id` and `upload_id` by `staging_id` — so `chunkFile()` /
`UPLOAD_CHUNK_SIZE` and the per-file chunk loop are reused as-is and only the request builder
differs. That reuse is why the 48 KiB ceiling above is load-bearing for two features, not one.

The attach UI is implemented: see [session-attach-ui.md](session-attach-ui.md). Note that its
behaviour has since diverged in one respect worth knowing if you read the two side by side — a staged
batch is **no longer** restricted to the host it was uploaded to; the session host fetches the bytes
across hosts when they differ.

## Tests

Units `randomId.test.ts` (6), `fileUploadChunks.test.ts` (5), `shellQuote`/`uploadProgress`; Cypress
`TerminalFileDropUpload` (4), `TerminalFileUploadFailure` (2), `MobileTerminalUploadButton` (2),
`TerminalFileUploadProgressFooter` (2), `TerminalDropInsertHostPath` (2, internal host-path drag),
`TerminalNativeFileDrop` (7: highlight, real paths, upload, folder refusal, routing to the terminal
under the pointer, a drop outside every terminal), `GhosttyTerminalSession` (a dropped path pasted
bracketed and plain); unit `sessionReadsLocalPaths.test.ts` (5).
The transport deadline itself is pinned in `packages/tddy-livekit-web/src/transport.test.ts`.
