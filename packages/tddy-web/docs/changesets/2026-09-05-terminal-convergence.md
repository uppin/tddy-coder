# 2026-09-05 — One terminal fed by the session connection

**Type:** Feature

`GhosttyTerminalSession` replaces `GhosttyTerminalLiveKit` (736 lines) and `GhosttyTerminalGrpc`
(631). It takes a `TerminalFeed` and nothing transport-specific: no `livekit-client` import, no
`Room`, no token. `SessionLiveKitTerminal` collapses into the one rendering path, and every call site
— `SessionRuntime`, `SessionMainPane`, `GrpcSessionTerminal`, `index.tsx`, the stories and the Cypress
drivers — renders it.

`SessionConnection.openTerminal(options)` opens the feed. The connection knows the host, the session
and the wire; `TerminalOptions` carries the four things it cannot know — which terminal, the
operator's `sessionToken` (the auth gate fills that field on unary calls only, and both terminal RPCs
are server-streaming), the initial grid, and `controlToken` as a **getter** read at send time, because
the control lease moves between screens and a snapshot goes stale into silent `failed_precondition`
refusals.

**A room-carried session has scrollback for the first time.** Its room serves the PTY and nothing
else, so `openRoomTerminalFeed` takes bytes from the session participant and **history from the host
daemon**, which is where the capture ring lives — no output byte leaves the room and no routing
changes. Because `terminal.TerminalOutput` is `bytes data` and carries no offsets, that fill is
**unanchored**: `TerminalHistoryForwardLoader`'s anchor becomes `bigint | null`, and `null` pages to
the capture tip ending on the first chunk reporting `atEnd`. An anchor of `0` keeps its opposite
meaning — the terminal captured nothing, so do not page. Termination is `atEnd` only; `atOldest` is
not a terminator, since the first chunk of a forward fill normally reports it while the fill continues.

`feedSupportsHistory(feed)` is the single place the scrollback question is asked, and it reads the
value rather than `"history" in feed` — a feed built conditionally carries the key either way.

Two defects were found and fixed during the work, both introduced by the convergence itself:

- `useDirectRoomTerminal` was called below an `if (error)` early return in `index.tsx`. As a component
  in JSX that was harmless; as a hook it meant the render that sets an error calls one fewer hook than
  the render before it, so React threw and painted nothing — on precisely the screen whose job is to
  report a token-fetch failure. Nothing caught it: `tddy-web` has no ESLint config, so
  `react-hooks/rules-of-hooks` never runs.
- The real terminal canvas painted over the "Claim terminal" mutex overlay, leaving a session another
  screen controls looking interactive while swallowing every key. Panes now state the ladder inline,
  bottom-up: terminal panes `2`, mutex overlay `3`, open agent conversation `4`.

Docs: [the session terminal](../terminal-session.md) · [session connections](../session-connections.md).

## Verification

- `bun run --filter tddy-web test:unit` — **1077 pass, 0 fail** (1442 assertions, 123 files).
- Cypress component — **102 passing** across the 17 specs this change touches. The five predecessor
  specs were re-pointed rather than dropped: 51 tests became 52, each mapping 1:1 to a renamed
  counterpart.
- `roomTerminalFeed` and `terminalFeed` carry 23 unit tests proven by 19 mutation checks — wiring
  history to the participant instead of the host, hoisting `controlToken`, neutralising the frame
  identity guard, pinning the replay mode, and dropping the ACK skip each fail a named test.
- `tsc --noEmit` is not a gate in this package: 521 errors, of which 129 are `Cannot find module
  'bun:test'` (the tsconfig declares no `@types/bun`, so every bun test file emits one).

The `ghostty-*` **e2e suite could not be run**. It needs `tddy-demo`, `tddy-demo-tui`, `tddy-coder`,
`tddy-daemon`, the `echo_terminal` example, a built web bundle and a Docker LiveKit server; with all of
those present, `tddy-demo` still fails its 15-second readiness probe under the `script` PTY wrapper, so
every spec dies in a `before all` hook before any web code loads. Those failures are not attributable
to this change — it contains no `.rs` or `.proto` file — and `ghostty-terminal-stories.cy.ts`, the one
ghostty spec needing no backend and exercising the stories this change rewired, passes 3/3.
