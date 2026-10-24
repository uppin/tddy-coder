# `StartPhase` — the steps of a session start

`StreamStartSession` (`session.proto`) reports the slow steps of a session start as they happen, so a
client can say what the host is doing while the start is in flight. Unary `StartSession` runs the same
start and reports nowhere.

`StartSessionEvent` is a `oneof` of six arms: `attachment_progress` (attachment materialisation, 1),
`result` (2), `phase` (a `StartPhase`, 3), `keep_alive` (4), `clone_progress` (5) and `started` (6). The
`result` is always the last event; a failed start ends the stream with the failure's status instead and
never emits one, and a cancelled start ends it `CANCELLED`.

## `StartTaskStarted`

`started` (field 6) is `StartTaskStarted { string task_id; string daemon_instance_id; }`, and it is
always the **first** event of a streamed start: the background task the start runs as, and the daemon
running it. `tasks.TaskService.CancelTask` with that `task_id` and `daemon_instance_id` cancels the
start ([background tasks § Session starts run as tasks](../../../docs/ft/daemon/background-tasks.md#session-starts-run-as-tasks)).
A split start carries a second `started` after it, relayed from the codebase host with that host's own
task. The unary start emits none.

## `StartPhase`

`StartPhase { Step step; Boundary boundary; }` — two enums rather than a message per step, so a
consumer tracks "the last BEGIN without its END".

| `Step` | The step |
|---|---|
| `STEP_CLONE` | cloning the session's project onto this host; only when the host has no checkout of it yet. Comes before `STEP_WORKTREE` |
| `STEP_WORKTREE` | fetching the project and cutting the session's git worktree |
| `STEP_SEMANTIC_INDEX` | building the session's semantic index over that worktree; only when the request asked for one |
| `STEP_AGENT` | launching the session's agent in the worktree |

`BOUNDARY_BEGIN` and `BOUNDARY_END` bracket each step. **A step that fails sends no END**: the stream
terminates with the failure, so the last BEGIN a consumer saw names the step that failed.

Every phase event reaches the client before the terminal event: the result never overtakes an END.

## `CloneProgress`

`clone_progress` (field 5) carries git's progress during `STEP_CLONE`, parsed from `git clone --progress`:

| Field | |
|---|---|
| `stage` | git's words for the stage: `Counting objects`, `Compressing objects`, `Receiving objects`, `Resolving deltas`, … (without a `remote: ` prefix) |
| `percent`, `objects_done`, `objects_total` | the `NN% (done/total)` part of the line |
| `bytes_received`, `bytes_per_second` | the `38.50 MiB \| 4.25 MiB/s` part, in bytes; 0 when the line carries none |
| `line` | the line as git wrote it, for display when no stage could be read |

Events are throttled at the source: the first line, every stage change, every 100%, and otherwise at most
one per 250 ms. They arrive between the step's BEGIN and END. A supervised host's clone sends none yet.

## `StartKeepAlive`

`StartSessionEvent` carries a further variant, `StartKeepAlive { string activity; }` (`keep_alive`, field 4).
It says the start is alive and waiting on `activity` (`"provisioning"` while a project's checkout is cloned,
which can stop to ask the operator for an SSH key passphrase), so a relay that ends a forwarded stream after
30 seconds of silence does not end this one. The host emits one every 5 seconds from the start of that wait
until it ends, and none after. A consumer ignores it: it is neither a phase nor the result. The unary start
emits none.

## Which starts report

claude-cli, cursor-cli and workspace starts report. The sandboxed claude-cli and cursor-cli starts, the
tool start and the split start report no phases. Children spawned by a PR-stack orchestrator or a
grill-me conversation discard their start's progress and report none either.

The host-side seam is `AttachmentProgressSink::begin_phase` / `end_phase` (and `keep_alive`, `clone_progress`)
([tddy-session-files](../../tddy-session-files/docs/host-documents-and-attachments.md#materialisation-progress));
the start code is described in
[tddy-session-lifecycle's session-service.md](../../tddy-session-lifecycle/docs/session-service.md#start-phases).
The web's rendering is in
[tddy-web's session-start-and-indexing-progress.md](../../tddy-web/docs/session-start-and-indexing-progress.md).
