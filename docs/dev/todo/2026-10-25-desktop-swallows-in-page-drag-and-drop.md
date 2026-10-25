# 2026-10-25 — Tddy Desktop swallows every in-page HTML5 drag-and-drop

**Category:** Known defect
**Source:** #765 (Finder drops onto Desktop session terminals), while finding its root cause

## What is broken

The Desktop window keeps Tauri's native drag-drop handler on (`dragDropEnabled`, the default), and on
macOS wry's handler (`WryWebView::draggingEntered` / `performDragOperation`) answers **every** drag
itself — it returns `true` for all of them and never forwards one to WebKit. So inside Tddy Desktop
the page receives no HTML5 `dragover` / `drop` at all, not only for drags from the OS:

- dragging a Files-tab entry onto a session terminal (the `HOST_PATH_MIME` route in
  `TerminalFileDropZone`) does nothing;
- dropping Finder files on the create-session attachments area (`AttachmentDropZone`) does nothing.

Both work in a browser.

## Why it was not fixed in #765

#765 fixed the one route that was asked for — Finder files onto a session terminal — by routing the
window's native drag events to the terminal under the pointer
(`components/connection/nativeFileDrop`). The handler has to stay on for that: it is the only source
of a dropped file's real path. These two routes were already broken before #765 and are outside its
scope.

## What would close it

- **Attachments area**: register `AttachmentDropZone` with `useNativeFileDropTarget`, as
  `TerminalFileDropZone` does, and read the dropped files through the window's bridge
  (`describe` / `read`) into the attachment flow.
- **In-page drags** (no file paths — the native relay already reports them as `enter` with
  `paths: []`): either a pointer-event drag for the Files-tab entry, or a Tauri-side passthrough that
  forwards drags carrying no file URLs to WebKit (wry's handler returning `false` for them).
