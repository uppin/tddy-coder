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

## Indexing indicator

`SessionIndexingIndicator` (`src/components/session/`) follows
`code_navigation.CodeNavigationService.WatchCodeIndex(session)` on the **host that owns the session**:
`SessionsDrawerScreen` resolves the `CodeNavigationService` client for the selected session's owning host
and passes it through `SessionMainPane` as `codeNavigationClient`, the same client the code pane's
navigation uses. `SessionMainPane` renders the indicator at the start of the session header row when that
client exists and a session is selected
(`data-testid="session-indexing-indicator"`).

| Stream | Shows |
|---|---|
| a progress message | `Indexing — <phase> <percentage>%` |
| a message carrying `ready` | nothing |
| a message carrying `error` | `Indexing failed — <reason>` |
| the stream ending with no message (nothing warmed the session) | nothing |
| the call failing | `Indexing failed — <message>` |

The indicator never blocks the session. It consumes the stream with a `cancelled` flag rather than an
`AbortSignal`, because the LiveKit transport accepts a signal for server-streaming calls and never reads
it; unmounting, or a change of client, token or session, ends the previous follow. Without a navigation
client the header shows no indicator.

## Testing

| Spec | Pins |
|---|---|
| `cypress/component/SessionStartAndIndexingProgress.cy.tsx` (2) | the create pane shows the current start phase; the header shows indexing until ready |
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
