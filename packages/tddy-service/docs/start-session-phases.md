# `StartPhase` — the steps of a session start

`StreamStartSession` (`session.proto`) reports the slow steps of a session start as they happen, so a
client can say what the host is doing while the start is in flight. Unary `StartSession` runs the same
start and reports nowhere.

`StartSessionEvent` is a `oneof` of eight arms: `attachment_progress` (attachment materialisation, 1),
`result` (2), `phase` (a `StartPhase`, 3), `keep_alive` (4), `clone_progress` (5), `started` (6),
`remote_call` (7) and `worktree_ready` (8). The
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

## `StartRemoteCall`

`remote_call` (field 7) is one command the start ran on its target (an SSH alias or a Coder
workspace), sent when that command ended:

| Field | |
|---|---|
| `host` | the target, as its transport names it (`ssh target remote-dev`) |
| `command` | the caller's command, never a transport's wrapper; cut to 200 characters, credentials scrubbed |
| `duration_ms` | how long the call took |
| `outcome` | `OUTCOME_EXITED`, `OUTCOME_TIMED_OUT` (still running at its deadline and stopped) or `OUTCOME_UNREACHABLE` (nothing ran) |
| `exit_code` | only for `OUTCOME_EXITED` |
| `stderr_tail` | the last 400 characters of stderr (or what the transport said) for a call that failed; empty otherwise; credentials scrubbed |

A credential is the user-info of a `scheme://user:password@host` URL, which reads `***`. A start on
this host sends none: only a target's calls are reported. A consumer that follows a start (the start
ledger) folds them into the start's log; a relay passes them on unchanged.

## `StartWorktreeReady`

`worktree_ready` (field 8) is `StartWorktreeReady { string branch; string worktree; }`, sent once a start
on a target has cut its worktree there: from that event on, the branch and the checkout exist on the
target whether or not the start goes on to succeed.

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

The host-side seam is `AttachmentProgressSink::begin_phase` / `end_phase` (and `keep_alive`, `clone_progress`,
`remote_call`, `worktree_ready`)
([tddy-session-files](../../tddy-session-files/docs/host-documents-and-attachments.md#materialisation-progress));
the start code is described in
[tddy-session-lifecycle's session-service.md](../../tddy-session-lifecycle/docs/session-service.md#start-phases).
The web's rendering is in
[tddy-session-create-web's start-progress.md](../../tddy-session-create-web/docs/start-progress.md).
