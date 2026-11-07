# Session start progress

What the create-session form shows while a session starts: the **start progress** list under the form,
cancelling a start, and the stream's keep-alive events. The feature is described in
[Session Drawer § Start progress](../../../docs/ft/web/session-drawer.md#start-progress). The session
header's indexing indicator is `tddy-web`'s ([indexing-indicator.md](../../tddy-web/docs/indexing-indicator.md)).

## Start progress

`CreateSessionPane` starts every session with `startSessionStreamed` from `useSessionAttachments`,
which consumes `SessionService.StreamStartSession` — with or without attachments — and ends with the
one `result` event that creates the session. The unary `startSession` is not used by the form.

`useSessionAttachments` exposes `startProgress: StartProgressState` beside the per-attachment
`progress`. `startProgress.ts` folds each non-attachment, non-result event into it
(`startProgressAfter`):

| Event | Effect |
|---|---|
| `phase` `BEGIN` | the step is appended in report order, `active` (a repeated `BEGIN` moves it to the end) |
| `phase` `END` | the step becomes `done`; ending `CLONE` also drops the clone progress |
| `cloneProgress` | replaces `clone` (stage, percent, bytes received, bytes per second) |
| `keepAlive`, anything else | nothing |

The state resets when a start begins. A stream that errors (or ends without a result) marks the step in
progress `failed` (`startProgressFailed`) and the list stays under the error; a start that creates the
session clears it. Steps appear **as the host reports them** — whether a start clones or indexes is the
host's decision — so there is no fixed list.

`CreateSessionStartProgress` renders the list under the form while Create is disabled
(`data-testid="create-session-start-progress"`), and nothing until the first step is reported. The
heading (`…-heading`) reads "Creating session on <host label>": the codebase host on a split start, the
session's host otherwise. Each row is `create-session-start-step-<name>` with `data-state` `active`,
`done` or `failed`:

| Step | Row |
|---|---|
| `CLONE` | Cloning repository |
| `WORKTREE` | Worktree |
| `SEMANTIC_INDEX` | Semantic index |
| `AGENT` | Agent |

While the clone is active its row carries `create-session-clone-progress`: git's stage and percent, a
`role="progressbar"` bar, and `bytes · throughput/s` through `formatAttachmentBytes` (binary units).

The wire contract is [`StartPhase`](../../tddy-service/docs/start-session-phases.md).

## Cancelling a start

The stream's first event is `started` (`StartTaskStarted { taskId, daemonInstanceId }`), the task the
start runs as. `startSessionStreamed(request, onStarted)` hands each `started` to `onStarted`; the latest
stands, so on a split start it is the codebase host's task, relayed after the agent host's.

`useCreateSessionSubmit` keeps that task and returns `startCancel` (`StartCancel` in
`CreateSessionActions.tsx`) when a `taskClient` is given: `enabled` once a task is known and no cancel
is in flight, `cancelling`, `cancelled`, and `cancel`, which calls
`taskClient.cancelTask({ sessionToken, taskId, daemonInstanceId })`. A stream that ends with
`Code.Canceled` sets `cancelled` instead of the error, and a refused `cancelTask` sets the error and
re-enables Cancel. `CreateSessionActions` routes Cancel to `startCancel.cancel` while submitting
(otherwise `onCancel`), labels it **Cancelling…** while one is in flight, and shows
`create-session-start-cancelled` ("Start cancelled") once a cancelled start has ended. The main variant
passes no `startCancel`.

`taskClient` is the TaskService on the host the form starts on. `useSessionsDrawerClients` gives the
selected daemon's, and `useSessionsDrawerOwningHost` the owning host's (`activeTaskClient`).
`SessionsDrawerScreen` hands `SessionMainPane` one of them by mode, like its other clients, and
`SessionMainPane` passes it on to `CreateSessionPane` and, through the workflow-view context, to
`PrStackScreen` → `CreateSessionDialog`.

## Keep-alive events

The stream can also carry `keepAlive` events while the host provisions the project. `startProgressAfter`
ignores them: a `keepAlive` is not mistaken for the result and changes no step.

## Testing

| Spec | Pins |
|---|---|
| `cypress/component/SessionStartAndIndexingProgress.cy.tsx` (1) | the create pane lists the start's steps |
| `cypress/component/CreateSessionStartProgress.cy.tsx` (5) | the step list in report order and ticked as each ends; the clone row's stage, percent, bytes and throughput updating in place; no clone row for a start that does not clone; the step a failed start stopped at, with the error; the host named in the heading |
| `cypress/component/CreateSessionAttachmentProgress.cy.tsx` | per-attachment progress, and a start with nothing attached going through the stream |
| `cypress/component/CreateSessionPane.cy.tsx` (29) | the form, started through the streamed start |

Component specs here start over the stream through `registerServerStreamFallback`
(`tddy-connectrpc-testkit`), registered in `cypress/support/component.ts` as `tddy-web` does — see
[indexing-indicator.md § Testing](../../tddy-web/docs/indexing-indicator.md#testing).
