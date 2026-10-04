# `StartPhase` — the steps of a session start

`StreamStartSession` (`session.proto`) reports the slow steps of a session start as they happen, so a
client can say what the host is doing while the start is in flight. Unary `StartSession` runs the same
start and reports nowhere.

`StartSessionEvent` is a `oneof` of three arms: `attachment_progress` (attachment materialisation),
`phase` (a `StartPhase`) and `result`. The `result` is always the last event; a failed start ends the
stream with the failure's status instead and never emits one.

## `StartPhase`

`StartPhase { Step step; Boundary boundary; }` — two enums rather than a message per step, so a
consumer tracks "the last BEGIN without its END".

| `Step` | The step |
|---|---|
| `STEP_WORKTREE` | fetching the project and cutting the session's git worktree |
| `STEP_SEMANTIC_INDEX` | building the session's semantic index over that worktree; only when the request asked for one |
| `STEP_AGENT` | launching the session's agent in the worktree |

`BOUNDARY_BEGIN` and `BOUNDARY_END` bracket each step. **A step that fails sends no END**: the stream
terminates with the failure, so the last BEGIN a consumer saw names the step that failed.

Every phase event reaches the client before the terminal event: the result never overtakes an END.

## Which starts report

claude-cli, cursor-cli and workspace starts report. The sandboxed claude-cli and cursor-cli starts, the
tool start and the split start report no phases. Children spawned by a PR-stack orchestrator or a
grill-me conversation discard their start's progress and report none either.

The host-side seam is `AttachmentProgressSink::begin_phase` / `end_phase`
([tddy-session-files](../../tddy-session-files/docs/host-documents-and-attachments.md#materialisation-progress));
the start code is described in
[tddy-session-lifecycle's session-service.md](../../tddy-session-lifecycle/docs/session-service.md#start-phases).
The web's rendering is in
[tddy-web's session-start-and-indexing-progress.md](../../tddy-web/docs/session-start-and-indexing-progress.md).
