# 2026-09-11 — The web moves to three session service clients

`src/gen/` gains `terminal_session_pb.ts`, `session_files_pb.ts` and `types_pb.ts`. The terminal
hooks and components bind `TerminalSessionService`; the file, context, upload and attachment hooks
bind `SessionFilesService`; `ConnectionService` keeps session lifecycle, tools, activity and
projects.

| Client | Call sites |
|---|---|
| `TerminalSessionService` | `useSessionTerminals.ts`, `useTerminalControl.ts`, `terminalHistoryLoader.ts`, `GrpcSessionTerminal.tsx`, `rpc/connections/terminalFeed.ts`, `rpc/connections/localHost.ts`, `rpc/connections/hostServedSession.ts`, `rpc/connections/livekit/roomTerminalFeed.ts`, `rpc/connections/livekit/sessionConnection.ts` |
| `SessionFilesService` | `SessionFilesTab.tsx`, `SessionWorkflowFilesModal.tsx`, `attachments/HostDocumentPicker.tsx`, `useSessionFileUpload.ts`, `useStagedAttachmentUpload.ts`, `useSessionAttachments.ts`, `workflowViews.tsx`, `CreateSessionPane.tsx`, `CreateSessionDialog.tsx` |

A connection memoises its client **per service**, so an unchanged route still yields one client per
coordinate rather than one overall.

The Cypress fakes are split the same way —
`cypress/support/rpc/terminalSessionServiceBackend.ts` and `sessionFilesServiceBackend.ts` beside
`connectionServiceBackend.ts`, following the shape `hostServiceBackend.ts` established. A spec
mounts only the backends its component talks to.

Cross-package proto generation cost **one manifest line**. `session_files_pb.ts` and `types_pb.ts`
needed none — both protos are already in the root this package generates from — and
`terminal_session_pb.ts` needed one entry for `../tddy-terminal-rpc/proto`, the multi-root support
`scripts/generated-code.manifest` already had and `packages/tddy-livekit-web` already proved.

**A web bundle and a daemon must be upgraded together.** Twenty-two coordinates moved, so an older
bundle cannot open a terminal, sync context or upload a file.

Docs: [session-attach-ui.md](../session-attach-ui.md),
[terminal-file-upload.md](../terminal-file-upload.md).
Full record: [../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md](../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md).
