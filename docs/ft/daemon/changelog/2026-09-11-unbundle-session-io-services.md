# 2026-09-11 — The terminal, context and session-file services

The daemon's terminals and its session files are services of their own.

**`terminal_session.TerminalSessionService`** carries every terminal: bidirectional I/O, the
anchored output stream, input with its cumulative offset, offset-addressed history, the three
multi-terminal lifecycle methods, and the single-screen control mutex. Nine methods, one
implementation, and both servers of the family — the daemon and a `tddy-coder` session's own
participant — register the same constructor rather than writing handlers. A session reached over
HTTP and the same session reached over LiveKit answer identically, and there is a test that opens one
session through both and says so.

That coordinate's proto already existed and was served nowhere, which is how the repo came to have
two terminal message sets and six converters between them. There is now one message set and no
converters.

**`session_files.SessionFilesService`** carries everything under a session's directory: the
allowlisted workflow files, the agent context manifest and its reads, terminal-drop uploads, the
pre-session staging area, and host-document fetches. Thirteen methods. Eight of them route to the
host that holds the bytes, and the routing decision sits on the served surface rather than on the
transport beneath it — so a request the daemon makes for itself obeys the host it named, and an
unauthenticated request can no longer drive an outbound forward.

`connection.ConnectionService` keeps session lifecycle, tools, agent activity and projects — 50
methods.

## For operators

**A `tddy-web` bundle and a daemon must be upgraded together.** Twenty-two coordinates moved, so an
older bundle cannot open a terminal, sync context or upload a file. This is the largest single break
in the `#unbundle` sequence.

**Sandboxed sessions gain terminal history and lose the eager buffer dump.** A jailed session's PTY is
now an ordinary terminal to the streaming bridge, which means three visible changes:
`GetTerminalHistory` answers with real offset-anchored chunks instead of `not_found`;
`StreamSessionTerminalIO` replays on attach with a prologue and an anchoring frame instead of starting
live; and `StreamTerminalOutput` in TAIL mode sends the mouse-mode prologue plus the last 8 KiB rather
than the whole retained buffer in 32 KiB frames. The scrollback is still reachable — paged on scroll-up,
which is how every non-sandboxed terminal has always behaved. The alternative was a sandbox-specific
arm in the one surface that is supposed to have none.

**The RPC Playground lists the terminal service but cannot describe it.** gRPC reflection serves
descriptors from a set compiled in `tddy-service`, and the terminal proto lives in
`tddy-terminal-rpc`. The service appears in the tree and its methods are not composable there.
Tracked in [`docs/dev/todo/`](../../../dev/todo/2026-09-11-reflection-cannot-serve-descriptors-for-protos-outside-tddy-service.md).

## Where the contract is documented

| Surface | Document |
|---|---|
| Terminals, the control mutex, lazy replay | [terminal-sessions.md](../terminal-sessions.md) |
| Agent context sync | [agent-context-sync.md](../agent-context-sync.md) |
| Start-session attachments and staging | [session-attachments.md](../../coder/session-attachments.md) |
| The session Files tab | [session-files-inspector.md](../../web/session-files-inspector.md) |
| Scroll-up history | [terminal-replay-lazy-scroll.md](../../web/terminal-replay-lazy-scroll.md) |
| The coder participant's coordinates | [session-participant-rpc.md](../../coder/session-participant-rpc.md) |
| Split placement's context reads | [remote-managed-worktree.md](../remote-managed-worktree.md) |
| The room's file surface | [session-room.md](../session-room.md) |
| Reflection and the playground | [rpc-playground.md](../rpc-playground.md) |

Implementation: [`tddy-terminal-rpc`](../../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md),
[`tddy-session-files`](../../../../packages/tddy-session-files/docs/session-files-service.md),
[`tddy-codegen`](../../../../packages/tddy-codegen/docs/tonic-adapter.md).
Engineering record: [`docs/dev/changesets/2026-09-11-unbundle-session-io-services.md`](../../../dev/changesets/2026-09-11-unbundle-session-io-services.md).

## Acceptance criteria, and two deviations

Met:

- `generate_tonic_adapter` emits a working tonic trait impl for unary, server-streaming (including
  the associated `…Stream` type), client-streaming and bidirectional methods, and the generated body
  calls the shared status conversion rather than constructing its own.
- `terminal_session.TerminalSessionService` serves all nine terminal methods over Connect-HTTP,
  LiveKit and the daemon's local UDS socket.
- `session_files.SessionFilesService` serves all thirteen methods of the four file families.
- `connection.ConnectionService` declares none of the 22 and is down to 50.
- No source converts between the two terminal message sets — six converters, not the two the plan
  counted, and a CI sweep across both packages enforces it.
- `tddy-coder`'s session participant serves the terminal family at the new coordinate, and a session
  reached over LiveKit and over HTTP answers identically for the seven methods both serve.
- The sandbox hop's terminal message resolves — by `sandbox.proto` owning its own
  `SandboxTerminalOutput` rather than by a re-pointed extern path, which is not possible without a
  dependency cycle.
- `tddy-web` opens a terminal, replays history, syncs context and uploads a file at the new
  coordinates.
- Per-package test baselines match, measured `--no-fail-fast` on both sides.

**Deviation 1 — "this node's two services reach the UDS socket through generated adapters."** Only
the **terminal** service does. `session_files` is deliberately **not** on that socket: nothing dials a
session-file method over it, checked across both in-jail binaries, so mounting it would have added
surface with no caller — which is the same failure this work spent a milestone removing in the other
direction. The terminal service is there because the jail provably dials it.

That also halves the arithmetic that justified building the generator here: the plan sized this at 22
hand-written adapter methods and the real figure was **9**, close to the ~9 its own reasoning had
dismissed as not worth a generator. What carries the decision is the shape rather than the count —
`StreamSessionTerminalIO` is the only bidirectional method in the entire 90-method surface, so a
generator built anywhere else would have handled two of the three shapes and been found incomplete
the first time it met a bidi one.

**Deviation 2 — "a generated adapter answers identically to a hand-written one for the same
service."** Not written, and it cannot be as scoped: the boundaries for this work forbid regenerating
the two hand-written adapters that already exist, so there is no hand-written adapter here that a
generated one replaces to compare against. The evidence instead is that the generated adapter serves
the jail's **live terminal traffic** on the UDS socket — remove the mount and the jail gets
`Unimplemented`. The comparison test is carried into the follow-up that replaces the two remaining
hand-written adapters:
[`docs/dev/todo/2026-09-11-node-ones-two-tonic-adapters-are-still-hand-written.md`](../../../dev/todo/2026-09-11-node-ones-two-tonic-adapters-are-still-hand-written.md).

## Known gaps

- The two full-stack daemon suites that would catch a sandbox-terminal regression end to end
  (`sandboxed_claude_cli_terminal_io_round_trips`,
  `sandboxed_session_streams_demo_tui_dimensions_in_terminal`) die on a pre-existing harness fault
  before reaching any terminal RPC, so they neither confirm nor deny the sandbox unification. A
  store-level parity suite built on a real `SandboxSessionState` covers it instead.
- A split agent still has no route to its own attachments. The move preserves the scope parameter
  that route will need.
- Staged batches consumed by a session start are still not garbage-collected; the restart-cleared
  staging root bounds only the abandoned case.
