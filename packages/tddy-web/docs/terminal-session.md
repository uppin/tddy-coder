# The session terminal (`src/components/GhosttyTerminalSession.tsx`, `rpc/connections/terminal.ts`)

One terminal component renders every session, on every wire. It takes a **terminal feed** — a byte
stream, and a history fetcher where one can be served — and knows nothing else. It imports no
`livekit-client`, constructs no `Room`, and mints no token.

What decides whether a terminal can scroll back is the feed, not the transport. A session carried
over its own LiveKit room and a session served by its host daemon render the same component, offer
the same scrollback, and differ only in which feed was opened for them.

Feature docs: [Web terminal](../../../docs/ft/web/web-terminal.md) ·
[Terminal replay and lazy scroll](../../../docs/ft/web/terminal-replay-lazy-scroll.md) ·
[Session terminal tabs](../../../docs/ft/web/session-terminal-tabs.md).

## The feed

```ts
interface TerminalFrame {
  readonly data: Uint8Array;
  readonly endOffset: bigint;   // zero on a live tail frame of an unstamped wire
  readonly atOldest: boolean;
  readonly snapshot?: boolean;  // a screen snapshot: one repaint, written as-is (it begins with RIS)
  readonly startOffset?: bigint; // set on every frame of a stream opened with a snapshot
}

interface TerminalStream {
  send(data: Uint8Array): void;
  onMessage(fn: (frame: TerminalFrame) => void): void;
  close(): void;
}

interface TerminalFeed {
  readonly stream: TerminalStream;
  readonly history?: TerminalHistoryFetcher;
  readonly ended?: Promise<void>;
}
```

`history` is optional because not every wire can replay. `ended` settles when the far end of the
stream is gone: the feed owns the read loop, so the end of a session is a fact only it holds, and the
component reads it rather than inferring one from silence.

**`feedSupportsHistory(feed)` is the one place that question is asked.** It reads the *value*, never
`"history" in feed` — a provider that builds its feed conditionally produces the key either way, and
a membership test would offer a scrollback control that calls `undefined`.

## `SessionConnection.openTerminal(options)`

A [session connection](session-connections.md) opens its own terminal. The connection already knows
the host, the session and the wire; `TerminalOptions` carries the four things it provably cannot:

| Option | Why the connection cannot know it |
|---|---|
| `terminalId` | a session has several terminals; `""` is the reserved main one |
| `sessionToken` | `createAuthGateInterceptor` fills this field on **unary** calls only, and the terminal RPCs stream (bidi `StreamSessionTerminalIO`, server-streaming `GetTerminalHistory`), so the operator's access token has to be threaded in |
| `controlToken` | a **getter**, read at send time — see below |
| `initialGrid` | the PTY resize before the snapshot, so the screen is serialized at the width the pane draws at; replaying at the wrong width is what garbled a 220-column buffer |

**`controlToken` is a getter, and that is load-bearing.** The control lease moves between screens: a
value snapshotted when the feed was built goes stale the moment another screen claims the terminal,
after which the daemon refuses every keystroke — and terminal input has no reply the component
renders. Reading it per send is what keeps a handover from turning into a terminal that looks alive
and accepts nothing.

## The two feeds

### `openRoomTerminalFeed` — `connections/livekit/roomTerminalFeed.ts`

Bytes travel `terminal.TerminalService/StreamTerminalIO`, addressed at the participant the session's
own process serves on — the only method that participant answers. Input is queued and drained into
one message per tick rather than one message per keystroke, so rapid typing and pastes cannot
overflow the data channel's send buffer. The stream is not opened until that participant is on the
roster, and a feed closed while still waiting deregisters its listener rather than leaving one on a
room nobody is watching.

**History does not travel that way, and this is the point of the design.** A session room serves the
PTY and nothing else, so scrollback has to be asked of the **host daemon**, which is where the
capture ring lives. The feed therefore takes its bytes from the session participant and its history
from the host — reaching past the room for that, and only that, without moving a single output byte
off it.

`controlToken` is not read here: `StreamTerminalIO` writes straight to the PTY handle the session's
own process holds, so there is no lease to compare against — room membership *is* the authorisation.

### `openDaemonTerminalFeed` — `connections/terminalFeed.ts`

The session's host, which both carries the stream and holds the capture. One bidi
`terminal_session.TerminalSessionService/StreamSessionTerminalIO` call carries the output down and the
keystrokes up, over RPC over LiveKit or RPC over Tauri IPC — the two providers every host connection
comes from, both able to carry a streaming request body.

**Every open asks for `SNAPSHOT`** — first connect, reconnect after a blip, remount. The opening
message names the terminal, presents the control token and the measured grid, and sets
`mode = SNAPSHOT`. The host answers with the screen as it is now: one snapshot frame
(`snapshot: true`), which begins with RIS so the live terminal writes it as-is, then only the bytes
written after it, every frame stamped with `startOffset` / `endOffset`. A re-open is therefore a fresh
VT shown the current screen at once — no resize, no SIGWINCH, no wait for the application to repaint,
and no byte resume to track. `TerminalStreamOffset` takes the absolute `endOffset` of every frame.

Each `send` becomes one stdin message on the request half, carrying a cumulative `input_offset`
counted by the feed and the control token read at that instant.

Every frame is stamped with the terminal it came from; one belonging to another terminal is dropped
rather than painted, and the stream stays open — a mis-routed frame says nothing about the frames
after it. An acknowledgement frame carries neither bytes nor an anchor and is skipped.

### Input refusals (`InputRefusalTracker`, `connections/inputDelivery.ts`)

The control lease bars writing, not reading: a pane opens and shows the terminal whether or not its
screen holds the lease (a watcher, or a screen re-opening after another took control), and only what
it types — keystrokes, pastes and its resize OSC — is refused.

The bidi stream has no per-send answer, so the host reports a dropped chunk on the output half: a
refusal frame carries `refusedInputOffset` (the chunk's cumulative input offset) and `inputRefusal`
(the reason, "terminal controlled by another screen"). `InputRefusalTracker` turns those frames into
`onInputDelivery` reports: a refusal raises **"Input not delivered"** with the reason, and the
indicator stays until an ACK reaches **past** the refused offset — proof a later chunk was applied. An
ACK at or below it clears nothing. The stream is not closed by a refusal, so re-claiming control
resumes typing on it. Both host terminal paths — `openDaemonTerminalFeed` and `GrpcSessionTerminal` —
use the tracker. The room's `StreamTerminalIO` reports nothing and never calls `onInputDelivery`.

## Anchored and unanchored fills

Scrollback is a forward fill: `TerminalHistoryForwardLoader` starts at the oldest retained byte and
pages forward until it reaches the live tail, appending into a second, read-only page terminal held
behind the live one.

The loader's anchor is `bigint | null`, and the distinction matters more than it looks:

- **A number** is the `endOffset` the open's first anchored frame reported — on a host terminal, the
  snapshot's `endOffset`, its sequence point. An anchor of `0` means the
  terminal has captured nothing, so there is nothing to page and the fill is done before it starts.
- **`null`** means the wire carries no offsets at all — `terminal.TerminalOutput` is `bytes data` and
  nothing more. There is no anchor to compare against, so the fill runs to the capture tip and ends on
  the first chunk that reports `atEnd`.

Collapsing the two onto `0` would page a terminal that has produced no output, and would leave a
room-carried session with no scrollback at all — which is precisely the gap this model closes.

Termination is `atEnd` only. `atOldest` is **not** a terminator: the first chunk of a forward fill
normally reports it while the fill continues, so ending on it would truncate every fill to one page.

## What the component still owns

The feed decides what can be done; the component does it. It keeps the live terminal pinned at
scrollback `0` — so native scrollback can never duplicate what the page terminal shows — and swaps
the two panes on a scroll-up-at-top gesture or the explicit affordance. It buffers incoming bytes
until ghostty-web is ready, which is the fix behind the 220-column reconnect garbling; routes the
wheel three ways depending on mouse tracking and alternate screen; and renders the status strip,
zoom bounds, mobile keyboard, shortcut drawer, file drop and upload that both of its predecessors
rendered identically.

Dropped paths go to the PTY through `pasteInput` — the terminal handle's `paste`, bracketed when the
program enabled DEC 2004 — rather than `sendInput`, so a program that tells pasted text from typed
text (Claude Code attaching a pasted image path) sees a drop as the paste a native terminal would
send. `readsLocalPaths` (from `SessionRuntime`, via `GrpcSessionTerminal` for gRPC terminals) is
handed to the drop zone, and decides whether a file dropped inside Tddy Desktop arrives as its real
path or is uploaded; see [terminal-file-upload.md](terminal-file-upload.md#tddy-desktop-drops-arrive-from-the-window-not-the-dom).

Panes state their stacking inline, bottom-up: terminal panes `2`, the terminal-control mutex overlay
`3`, an open agent conversation `4`. A CTA that claimed its layer only through utility classes would
be painted over by the terminal canvas, leaving a session another screen controls looking interactive
while swallowing every key.

## The tab strip (`components/sessions/SessionTerminalTabs.tsx`)

A session's extra terminals are tabs beside the Agent tab. `useSessionTerminals` holds them as
`{ terminalId, kind }[]`, seeded from `ListTerminalSessions` on attach, and `open(kind)` calls
`StartTerminalSession` with that `kind` (`"bash"` or a CLI harness's session type) and focuses the
result; a refusal lands in `sessions-terminal-error` and opens no tab. Tab labels are built in tab
order as the kind's name plus a per-kind ordinal ("Bash 1", "Warp 1"), by `terminalKindLabel`.

The `+` is `NewTerminalMenu.tsx`: a button and a `role="menu"` list of bash followed by
`CLI_HARNESS_OPTIONS`, rendered in its own wrapper outside the tab scroller so the scroller cannot
clip the menu. Its Escape and outside-press listeners are attached only while the menu is open. A
`sessionType` of `"tool"` (passed from `SessionRuntime`) limits it to bash; an unset one offers every
harness. `cliHarnessOptions.ts` is the one list both the menu and the Create pane's
`CreateSessionTypeToggle` read (`SESSION_TYPE_OPTIONS`, `CLI_HARNESS_OPTIONS`, `BASH_KIND`,
`TOOL_SESSION_TYPE`, `terminalKindLabel`).

In Cypress, `terminalSessionServiceBackend.ts` records `startedTerminalKinds` and lists each started
terminal with its requested kind; `sessionTerminalTabsPage.ts` offers `newTabMenu()` and
`newTabOption(kind)`. Feature doc: [Session terminal tabs](../../../docs/ft/web/session-terminal-tabs.md).

## Current limits

- **Two host terminal paths build the same stream.** `SessionRuntime` opens a terminal through the
  connection only for a connection that carries media, which a host-served session never does, so
  `GrpcSessionTerminal` (the host-served Agent and bash panes) builds its own bidi
  `StreamSessionTerminalIO` call with `SNAPSHOT` beside `openDaemonTerminalFeed`. It does so because it
  accounts un-acknowledged input, and a `TerminalFrame` has nowhere to carry a daemon's input
  acknowledgement; routing that path through the feed would silently drop the enqueued-input overlay.
  The two keep identical request queues (open message, then one message per send). Leftover
  resume-by-offset plumbing — `TerminalResumePoint`, `GhosttyTerminalSession.onOffsetUpdate` — is
  written but no longer read in production (backlog:
  `2026-10-25-web-host-terminal-feed-leftovers-after-snapshot`).
- **A host terminal that falls behind is not resynced.** If the host's broadcast drops frames for a
  slow subscriber, later offsets are wrong until `#term-snapshot` 2/4
  ([#761](https://github.com/uppin/tddy-coder/pull/761)) resyncs with a fresh snapshot.
- **Jailed terminals refuse the snapshot open** (`FAILED_PRECONDITION`) until `#term-snapshot` 3/4
  ([#762](https://github.com/uppin/tddy-coder/pull/762)) tells the host their size.
- **A room-carried session's frames carry no offsets**, so its fill is unanchored and its reconnect
  is not shown a snapshot the way the host path is (`#term-snapshot` 4/4,
  [#763](https://github.com/uppin/tddy-coder/pull/763)). Adding offsets to `terminal.TerminalOutput` would remove
  the distinction entirely.
