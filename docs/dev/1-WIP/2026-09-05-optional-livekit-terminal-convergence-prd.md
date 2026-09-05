# PRD: one terminal fed by the session connection

**Stack:** `optional-livekit` — node 5 of 7 (`terminal-convergence`)
**Target PRD on wrap:** [`docs/ft/web/web-terminal.md`](../../ft/web/web-terminal.md),
[`docs/ft/web/terminal-replay-lazy-scroll.md`](../../ft/web/terminal-replay-lazy-scroll.md)
**Date:** 2026-09-05

## Problem

There are two terminals: `GhosttyTerminalLiveKit` (736 lines) and `GhosttyTerminalGrpc` (631). They
render the **same chrome** — `GhosttyTerminal`, `ConnectionTerminalChrome`,
`TerminalConnectionStatusBar`, `ShortcutDrawer`, `MobileTerminalKeyboard`, `TerminalFileDropZone`,
`TerminalUploadButton` — and differ in two ways:

1. **How bytes arrive.** The LiveKit variant builds a `TerminalService` client through
   `useLiveKitTransportFactory` and, today, owns the `Room` connect and the token refresh itself. The
   gRPC variant takes an already-abstract `GrpcStream` (`send` / `onMessage` / `close`).
2. **What each can do.** Only the gRPC variant has scrollback history: `TerminalHistoryForwardLoader`,
   `TerminalStreamOffset`, a `HistoryFetcher`, and a 50 000-line page terminal. The LiveKit variant
   has none of it — a LiveKit session cannot scroll back past what is live.

Node 3 removed the first difference's root cause: a `SessionConnection` now owns the room, the
identity and the token, so the LiveKit terminal no longer has any reason to connect anything. What is
left is one component's worth of behaviour spread across two files, with the better feature set on
the path fewer users are on.

## What this PR delivers

**One** terminal component, fed by a `SessionConnection`, with history on every transport.

### The model

```ts
/** A session's terminal byte stream, however it is carried. */
interface TerminalStream {
  send(data: Uint8Array): void;
  onMessage(fn: (frame: TerminalFrame) => void): void;
  close(): void;
}

interface TerminalFrame {
  data: Uint8Array;
  endOffset: bigint;   // zero on live tail frames
  atOldest: boolean;
}

type HistoryFetcher = (fromOffset: bigint, untilOffset: bigint) => Promise<HistoryChunk | null>;
```

`SessionConnection` opens a `TerminalStream` and, where the transport supports it, a
`HistoryFetcher`. The component takes both and knows nothing else.

### Acceptance criteria

1. One component — `GhosttyTerminalSession` — replaces both. `GhosttyTerminalLiveKit` and
   `GhosttyTerminalGrpc` are deleted, not deprecated.
2. It takes a `TerminalStream` (+ optional `HistoryFetcher`) and never constructs a `Room`, mints a
   token, or imports `livekit-client`.
3. **Scrollback history works on a LiveKit session**, which it does not today: the
   `TerminalHistoryForwardLoader` / `TerminalStreamOffset` path applies to every transport whose
   connection offers a `HistoryFetcher`.
4. Every behaviour currently pinned by the LiveKit terminal's tests survives: connection status
   strip visibility (`shouldShowVisibleLiveKitStatusStrip`), reconnect handling, resize, zoom bounds,
   file drop and upload, mobile keyboard, shortcut drawer, key description.
5. Every behaviour currently pinned by the gRPC terminal's tests survives: byte buffering on
   reconnect (the 220-column garbling regression), offset alignment, forward history paging,
   `PAGE_SCROLLBACK`.
6. The existing Cypress specs for both terminals — including the e2e `ghostty-*` suite — pass against
   the single component, adapted only where they name the old component.
7. `SessionLiveKitTerminal.tsx` (95 lines) collapses into the one rendering path.

### Non-goals

Changing the terminal protocol, the daemon's PTY bridge, or `GhosttyTerminal` itself (the xterm-level
widget). Adding IPC. See the changeset's `## Boundaries`.

## Why this shape

- **The stream abstraction already exists.** `GrpcStream` is exactly the right interface; it just has
  the wrong name and only one implementation. Promoting it is less invention than it looks.
- **History is the user-visible win.** This node would be a pure refactor if it only deleted a file.
  Giving LiveKit sessions scrollback is why it is worth doing as its own PR, and it is what makes it
  reviewable as a behaviour change rather than a diff to eyeball.
- **After node 3, the LiveKit terminal's extra responsibility is dead weight.** It connects a room
  the connection already owns. Leaving two components would leave that duplication live.

## Constraints

- **Zero new npm dependencies** (no public npm registry; `bun run local-registry-install`).
- `tddy-web` only — no proto and no daemon change. Where a transport cannot serve history, the
  connection offers no `HistoryFetcher` and the component degrades to live-tail only.
- The e2e `ghostty-*` suite is **not** in the CI gate (`docs/dev/guides/ci.md`), so it must be run
  locally and the result reported.

## Successor PRs

None — this is a leaf. `feature/optional-livekit/desktop-ipc-host` is its sibling off the same base.
