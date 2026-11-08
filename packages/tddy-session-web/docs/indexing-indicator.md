# Indexing indicator

What the web shows while a session's code index loads: the **indexing indicator** in the session header
and the Code pane's strip. The feature is described in
[Worktree Code Pane § Indexing indicator](../../../docs/ft/web/session-code-pane.md#indexing-indicator).
The start progress list under the create form is `tddy-session-create-web`'s
([start-progress.md](../../tddy-session-create-web/docs/start-progress.md)).

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
| `cypress/component/SessionIndexingIndicatorAcceptance.cy.tsx` (1) | the header shows indexing until ready |
| `cypress/component/SessionIndexingIndicatorResubscribe.cy.tsx` (4) | the header shows a load that starts after its first watch ended empty; drops what it said once its watch ends mid-load; watches again after 1, 2 and 4 s; a refused watch is not repeated and names why |
| `cypress/component/CodeIndexStatusStrip.cy.tsx` | the strip's own states: the failure's reason; an index let go while the pane is open reads as starting; reconnecting with why; a worktree the host serves no language of has no code index, and why. Live progress, phases and `+N more` are pinned by `WorktreeCodePaneCodeSession.cy.tsx` ([code navigation](code-navigation.md#testing)) |
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
