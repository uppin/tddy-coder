# 2026-10-25 — Native drops in Tddy Desktop, and dropped paths delivered as a paste

**Type:** Fix

PR [#765](https://github.com/uppin/tddy-coder/pull/765). Cross-package entry: [`docs/dev/changesets/2026-10-25-desktop-terminal-file-drop.md`](../../../../docs/dev/changesets/2026-10-25-desktop-terminal-file-drop.md)

`components/connection/nativeFileDrop.tsx` (`NativeFileDropProvider`, `useNativeFileDropTarget`, `droppedFileUploadSource`), mounted in `index.tsx` inside the host application only; `TerminalFileDropZone` registers, highlights, and pastes real paths / uploads / refuses folders (`terminal-drop-refused`); `useSessionFileUpload.uploadSources` over `UploadSource`; `GhosttyTerminalHandle.paste` and `GhosttyTerminalSession`'s `pasteInput` for drops; `lib/sessionReadsLocalPaths.ts`, computed in `SessionRuntime`. Detail: [terminal-file-upload.md](../terminal-file-upload.md), [terminal-session.md](../terminal-session.md).
