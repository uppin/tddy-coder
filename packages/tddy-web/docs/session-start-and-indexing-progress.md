# Session start and indexing progress

What the web shows while a session starts and while its code index loads: the **start phase** under the
create form, and the **indexing indicator** in the session header. The feature is described in
[Session Drawer § Start progress](../../../docs/ft/web/session-drawer.md#start-progress) and
[Worktree Code Pane § Indexing indicator](../../../docs/ft/web/session-code-pane.md#indexing-indicator).

## Start phase

`CreateSessionPane` starts every session with `startSessionStreamed` from `useSessionAttachments`,
which consumes `SessionService.StreamStartSession` — with or without attachments — and ends with the
one `result` event that creates the session. The unary `startSession` is not used by the form.

`useSessionAttachments` exposes `startPhase: StartPhase_Step | null` beside the per-attachment
`progress`. It follows the stream's `phase` events: a `BEGIN` sets the step, an `END` clears it when it
names the step currently set, and the phase is cleared when the stream is over however it ended. A step
that fails sends no `END` (the stream errors instead), so the clearing in `finally` is what removes its
text. `null` also covers a start that is not being streamed.

`CreateSessionStartPhase` renders the line under the form while Create is disabled
(`data-testid="create-session-start-phase"`), and nothing between steps:

| Step | Text |
|---|---|
| `WORKTREE` | Creating worktree… |
| `SEMANTIC_INDEX` | Indexing (semantic)… |
| `AGENT` | Starting agent… |

The wire contract is [`StartPhase`](../../tddy-service/docs/start-session-phases.md). Starts that report
no phases (sandboxed, tool and split starts) show no line.

## Keep-alive events

The stream can also carry `keepAlive` events while the host provisions the project. The loop in
`startSessionStreamed` is an `if` / `else if` chain over `attachmentProgress`, `result` and `phase` with no
`else`, so a `keepAlive` is ignored: it is not mistaken for the result and sets no phase.

## Indexing indicator

`useCodeIndexProgress.ts` (`src/components/session/`) classifies one `CodeIndexProgress` message into a
`CodeIndexStatus` (`codeIndexStatusOf`) for both the header and the Code pane's strip:

| Status | From |
|---|---|
| `waiting` | nothing heard yet |
| `idle` | `state` `IDLE` — nothing holds the worktree's index; or the header's watch ended and the next has not reported |
| `none` | `state` `NONE` — the worktree has no code index: it is no Rust worktree and holds no file of an installed extension language with a configured server; `error` says why |
| `progress` | `state` `LOADING` — the message, naming the phase reported most recently, its percentage, and every running phase (`phases`) |
| `ready` | `state` `READY` |
| `failed` | `state` `FAILED` (its `error`, or *the code index failed* when empty), or the stream refused |
| `reconnecting` | the Code pane's `CodeSession` dropped and is being reopened (never the header) |

A message with `state` `UNSPECIFIED` comes from a host that predates the field and is read from
`error` and `ready`. `currentPhaseHeadline` renders `<phase> <n>%`; `phaseHeadline` renders one running
phase as `<title> <n>%`, or the bare title when it never reported a percentage.

### Session header

`useCodeIndexProgress(client, sessionToken, sessionId)` follows
`code_navigation.CodeNavigationService.WatchCodeIndex(session)` through a `CodeIndexWatcher`. The host
never starts anything for this watch: with no index daemon running, or a worktree that is not Rust or
does not exist yet, the watch ends at once with no message, and it also ends when the index daemon stops.
The watcher therefore **watches again** after every end or retryable failure, on the shared
`StreamReopener` schedule (`streamReopening.ts`: 1 s doubling to 30 s, back to 1 s once a watch has
delivered and stayed open 10 s), which is what lets the header show a load the Code pane or the agent
starts later. A watch that ended reports `idle` — what it said last is no longer known — and one that
failed retryably reports its reason, until the next watch reports. A **refusal**
(`INVALID_ARGUMENT`, `NOT_FOUND`, `PERMISSION_DENIED`, `UNAUTHENTICATED`, `FAILED_PRECONDITION`,
`UNIMPLEMENTED`) is not watched again and reads `failed` with the RPC's own message (no `[code]`
prefix). Unmounting, or a change of client, token or session, stops it.

`SessionIndexingIndicator` follows it on the **host that owns the session**:
`SessionsDrawerScreen` resolves the `CodeNavigationService` client for the selected session's owning host
and passes it through `SessionMainPane` as `codeNavigationClient`, the same client the code pane uses.
`SessionMainPane` renders the indicator at the start of the session header row when that client exists
and a session is selected (`data-testid="session-indexing-indicator"`).

| Status | Shows |
|---|---|
| `progress` | `Indexing — <phase> <percentage>%` |
| `failed` | `Indexing failed — <reason>` |
| anything else | nothing |

The indicator never blocks the session. The LiveKit transport accepts a signal for server-streaming calls
and never reads it, so the watcher drops whatever arrives after `stop` with a `stopped` flag and aborts
the signal for transports that do read it. Without a navigation client the header shows no indicator.

### Code pane strip

`CodeIndexStatusStrip` is the bottom strip of the [Code pane](code-navigation.md)
(`data-testid="worktree-code-index-status"`, with the status kind in `data-state`). It reads the status of
the pane's own `CodeSession` (`useCodeSession`, see [code navigation](code-navigation.md#the-code-session)),
so `WorktreeCodePane` renders it whenever it has a navigation client. Unlike the header it stays visible
in every state:

| Status | Summary (`worktree-code-index-status-summary`) | Detail (`worktree-code-index-status-detail`) |
|---|---|---|
| `waiting` | `Code index: checking…` | — |
| `idle` | `Code index starting…` (an open pane warms its index, so an idle root is about to load) | — |
| `none` | `No code index for this worktree` | why |
| `progress` | `Indexing — <phase> <percentage>%` | that phase's `message` (the latest-begun phase of that title), truncated with the full text as its title |
| `ready` | `Code index ready` | — |
| `failed` | `Indexing failed — <reason>`, in the error colour | — |
| `reconnecting` | `Reconnecting…` | why the session dropped |

While loading with more than one running phase the strip adds `+<N> more`
(`worktree-code-index-status-more`), whose `title` lists every running phase in begin order as
`<title> <n>%` followed by ` — <message>` when it has one.

## Testing

| Spec | Pins |
|---|---|
| `cypress/component/SessionStartAndIndexingProgress.cy.tsx` (2) | the create pane shows the current start phase; the header shows indexing until ready |
| `cypress/component/SessionIndexingIndicatorResubscribe.cy.tsx` (4) | the header shows a load that starts after its first watch ended empty; drops what it said once its watch ends mid-load; watches again after 1, 2 and 4 s; a refused watch is not repeated and names why |
| `cypress/component/CodeIndexStatusStrip.cy.tsx` | the strip's own states: the failure's reason; an index let go while the pane is open reads as starting; reconnecting with why; a worktree the host serves no language of has no code index, and why. Live progress, phases and `+N more` are pinned by `WorktreeCodePaneCodeSession.cy.tsx` ([code navigation](code-navigation.md#testing)) |
| `cypress/component/CreateSessionAttachmentProgress.cy.tsx` | per-attachment progress, and a start with nothing attached going through the stream |
| `cypress/component/CreateSessionPane.cy.tsx` (29) | the form, started through the streamed start |
| `cypress/component/ServerStreamFallback.cy.tsx` (7) | the testkit fallback below |

**`registerServerStreamFallback` (`tddy-connectrpc-testkit`).** The form always starts over the stream, so
a component spec that stubs only the unary `startSession` would leave it unanswered. The testkit offers
`registerServerStreamFallback({ unary, stream, toEvent })`: for every in-memory backend built afterwards
(process-wide), a backend that serves the unary method but not the stream answers the stream by running
the unary handler and emitting `toEvent(answer)` as its only event — a thrown `ConnectError` becomes the
stream's error, and the call is recorded as the unary call would be. A stream the backend implements
itself always wins. `cypress/support/component.ts` registers
`SessionService.startSession` → `streamStartSession` with the answer as the `result` event;
`ServerStreamFallback.cy.tsx` drives the testkit directly (the testkit package has no runner of its own)
and pins both registration paths (`onUnary` and `implement`), an explicit stream winning, the error code,
the recording and the no-fallback case.
