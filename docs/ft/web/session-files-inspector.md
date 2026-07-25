# Session Files — Uploaded files in the Inspector, draggable to the terminal

**Component:** `SessionInspectorDrawer` → new **Files** tab (`packages/tddy-web/src/components/sessions/`)
**Updated:** 2026-07-25
**Status:** Implemented

## Overview

Add a **Files** tab to the Session Inspector that lists the files already uploaded to the selected
session and makes them **repeatedly usable** — instead of a file's host path being typed once at
drop time and then lost.

Today the [terminal file drop upload](web-terminal.md) writes each dropped file to
`{session_dir}/uploads/{upload_id}/{file_name}` and types the absolute host path into the terminal
**once**. There is no record of what was uploaded, so re-referencing a file means re-dropping it.

The Files tab closes that gap:

1. **List** every uploaded file for the session (name, size, when), read from the host's uploads
   directory — so the list survives page reloads and is visible from any device viewing the session.
2. **Reuse** a file by getting its host path back into the terminal, either by **dragging** the file
   from the tab onto the terminal (desktop) or **tapping** it (mobile) to insert the path.
3. **Manage** files: copy the host path, insert it explicitly, or delete the upload.

Because the Inspector is an overlay that sits **on top of** the terminal, dragging a file toward the
terminal would drop onto the Inspector itself. So **starting a drag (or a tap-to-insert) auto-closes
the Inspector**, exposing the terminal underneath as the drop target.

## Which files

The tab lists the contents of the session's uploads root `{session_dir}/uploads/`, which the
existing upload flow populates as `{session_dir}/uploads/{upload_id}/{file_name}` (one `upload_id`
subfolder per drag gesture). The tab presents a **flat** list of files across all `upload_id`
folders — the grouping is an implementation detail of collision-avoidance, not something the user
cares about — sorted **newest first** by file modification time.

An empty uploads directory (or none yet created) renders an empty state, not an error.

## Listing — new `ListSessionUploads` RPC

Uploaded files live on the host filesystem, so the list is read server-side (chosen over a
browser-local registry so it survives reloads and is consistent across devices/tabs viewing the same
session). A new unary RPC mirrors the auth + session-dir resolution of `UploadSessionFileChunk`:

- Validates `session_token` and resolves the session's uploads root exactly as the upload path does.
- Walks each `uploads/{upload_id}/` subfolder one level deep, emitting one entry per regular file.
- A missing uploads root yields an **empty** list (not an error) — a session that never had an
  upload is a normal case.
- Entries are sorted newest-first by modification time.

The tab loads on mount and reloads after a successful delete. No streaming or polling — uploads
change only in response to user action (a drop, or a delete), and a drop already runs in the same
app, so an explicit reload after those events is sufficient.

## Delete — new `DeleteSessionUpload` RPC

A second unary RPC removes a single uploaded file, addressed by its `upload_id` + `file_name`:

- Both segments are untrusted client input and are validated as safe basenames, then a
  canonicalize-and-contain guard confirms the target resolves inside the session's uploads root —
  the same guard `write_upload_chunk` already applies (shared helper).
- Removes the single file. If its `upload_id` folder is now empty, the empty folder is pruned too so
  the uploads root does not accumulate stale directories.
- Deleting a file that does not exist returns `NotFound`.

UI: two-step **Delete → Confirm delete** (mirrors the Worktree tab's confirm), then the list
reloads.

## Reuse — drag (desktop) and tap (mobile), with Inspector auto-close

A file's host path gets back into the focused terminal by two routes, both of which **close the
Inspector** first so the terminal is reachable:

- **Desktop drag:** each file row is `draggable`. On `dragstart` the row writes its **host path**
  into the drag's `DataTransfer` under a private MIME type
  (`application/x-tddy-host-path`) and fires the tab's close-inspector callback. The
  [`TerminalFileDropZone`](web-terminal.md) is extended to recognize a drop carrying that type and
  **insert the quoted host path** into the terminal — it does **not** re-upload (the file is already
  on the host). An OS file drag (carrying `DataTransfer.files`) keeps its existing upload behavior;
  the two are distinguished by which data the drop carries.
- **Mobile / click:** touch devices do not fire HTML5 drag events, so tapping a row (or its
  **Insert** button) inserts the file's host path into the focused terminal and closes the
  Inspector. This click path works on desktop too as a non-drag alternative.

Inserting a path types the shell-escaped host path with a trailing space and **no newline** —
identical to what a native terminal file-drag produces, reusing `joinQuotedPaths` /
`shellQuote` (already used by the upload flow).

### Auto-close semantics

- The Inspector closes (`state → "closed"`) the moment a drag starts or a tap-to-insert fires. It
  does **not** auto-reopen after the drop/insert — the user reopens it via the Inspector toggle, the
  same as any other close. This keeps the interaction predictable and matches the manual close model
  already in `inspectorState`.
- Auto-close is driven by the same close path the header ✕ uses (`onClose`), so it also cancels the
  "auto-open on select" bookkeeping (`inspectorAutoOpenRef`) — a subsequent reconnect will not
  re-pop the Inspector over the terminal the user is dropping onto.

## Row actions

Each file row offers, besides drag/tap-to-insert:

- **Copy host path** — copies the absolute host path to the clipboard (via the existing
  insecure-origin-safe clipboard helper; the app is often served over plain http on a LAN address).
- **Insert into terminal** — explicit button; inserts the host path into the focused terminal and
  closes the Inspector (the click route above).
- **Delete** — two-step confirm; calls `DeleteSessionUpload` and reloads the list.

## Layout

```
┌──────────────────────────────────────────────────────┐
│ Details | Tools | Usage | Worktree | Files | VNC | …  │
├──────────────────────────────────────────────────────┤
│ report.pdf            1.2 MB   2 min ago               │
│   ⠿ drag   [Insert]  [Copy path]  [Delete]            │
│ diagram.png          340 KB    5 min ago               │
│   ⠿ drag   [Insert]  [Copy path]  [Delete]            │
└──────────────────────────────────────────────────────┘

empty state:
┌──────────────────────────────────────────────────────┐
│ No files uploaded to this session yet.                │
│ Drop files on the terminal to upload them.            │
└──────────────────────────────────────────────────────┘
```

## Protocol

```protobuf
service ConnectionService {
  // … existing …
  rpc ListSessionUploads(ListSessionUploadsRequest) returns (ListSessionUploadsResponse);
  rpc DeleteSessionUpload(DeleteSessionUploadRequest) returns (DeleteSessionUploadResponse);
}

message ListSessionUploadsRequest {
  string session_token = 1;
  string session_id = 2;
}
message ListSessionUploadsResponse {
  repeated SessionUploadEntry uploads = 1;  // newest first
}
message SessionUploadEntry {
  string upload_id = 1;      // per-drop UUID subfolder
  string file_name = 2;      // basename
  string host_path = 3;      // absolute host path
  uint64 size_bytes = 4;
  int64 uploaded_at_ms = 5;  // file mtime, unix milliseconds
}

message DeleteSessionUploadRequest {
  string session_token = 1;
  string session_id = 2;
  string upload_id = 3;  // untrusted — validated as a basename, contained under uploads root
  string file_name = 4;  // untrusted — validated as a basename
}
message DeleteSessionUploadResponse {}
```

## Backend

- New daemon module `session_uploads.rs` (sibling of `session_file_upload.rs`), reusing that
  module's segment-validation + canonicalize-and-contain guard (extracted to a shared helper):
  - `list_uploads(sessions_base, session_id) -> Result<Vec<UploadEntry>, Status>` — walks
    `uploads/*/*`, one level deep per `upload_id`; missing root ⇒ empty vec; sorted newest-first.
  - `delete_upload(sessions_base, session_id, upload_id, file_name) -> Result<(), Status>` —
    validates both segments, removes the file, prunes the emptied `upload_id` folder; `NotFound` for
    a missing file; `InvalidArgument` for an unsafe segment (writing/removing nothing outside root).
- `ConnectionServiceImpl` gains `list_session_uploads` / `delete_session_upload` handlers mirroring
  `upload_session_file_chunk`'s auth + `sessions_base` resolution.

## Frontend

- `InspectorTab` union gains `"files"`; `InspectorTabs` gains a **Files** button;
  `SessionInspectorDrawer` renders `SessionFilesTab` for that tab and passes down an
  `onInsertPathIntoTerminal(path)` and reuses `onClose` for auto-close.
- `SessionFilesTab` — lists uploads (`ListSessionUploads` on mount), renders draggable rows with
  Insert / Copy path / Delete (two-step), calls `DeleteSessionUpload` and reloads on success. On
  `dragstart` sets the private host-path MIME and closes the Inspector; on Insert/tap inserts the
  path and closes the Inspector.
- `TerminalFileDropZone` — recognizes a drop carrying `application/x-tddy-host-path` and inserts the
  quoted path (no upload); an OS file drop keeps the upload flow. The drag overlay shows for both.
- The focused terminal's text-insert is routed from the drawer up to the focused `SessionRuntime`'s
  terminal input (a focused-terminal insert hook on the runtime registry), so the click/tap route
  can reach it. The drag route inserts through the drop zone directly and needs no such routing.

## Scope

- **In scope:** Files tab; `ListSessionUploads` + `DeleteSessionUpload` RPCs and the
  `session_uploads` daemon module; flat newest-first listing across `upload_id` folders; drag
  (desktop) + tap/Insert (mobile) reuse routing the host path into the focused terminal; Inspector
  auto-close on drag/insert; Copy path; two-step Delete; extending `TerminalFileDropZone` to insert
  an already-uploaded host path without re-uploading.
- **Out of scope:** file **preview**/download in the tab; renaming; multi-select bulk actions;
  grouping the list by drag gesture; a streaming/polling live-refresh of the list; uploads for
  **remote** sessions beyond what the existing upload flow already supports (local daemon parity);
  deleting an entire `upload_id` group in one action.

## Related documentation

- [Web Terminal](web-terminal.md) — the file-drop upload flow this tab reuses and extends
  (`UploadSessionFileChunk`, `TerminalFileDropZone`, `joinQuotedPaths`).
- [Session Worktree inspector](session-worktree-inspector.md) — sibling Inspector tab; the two-step
  confirm pattern reused by Delete.
- [Session drawer](session-drawer.md) — Inspector host, overlay/docked layout, and
  `inspectorState` close semantics reused for auto-close.
- Daemon module: [`session_file_upload`](../../../packages/tddy-daemon/src/session_file_upload.rs)
  — the upload writer whose validation guard `session_uploads` shares.
