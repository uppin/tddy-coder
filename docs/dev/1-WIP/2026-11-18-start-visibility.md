# Changeset: A failing start on a target says which command failed, and what it left behind

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — several row-4 signals: four crates (`tddy-remote-shell`, `tddy-tool-engine`, `tddy-agent-launch`, `tddy-daemon-rpc`), a contract change to `GetSessionStart`'s JSON (and possibly `session.proto`'s `StartSessionEvent`), ordering of events into the ledger
**Stack**: `#coder-start` 8/10 — branch `feature/coder-start/start-visibility`, base `feature/coder-start/target-agent-start`

## Initial Discovery

[`2026-11-18-start-visibility-initial-discovery.md`](2026-11-18-start-visibility-initial-discovery.md)

## Prerequisites

- ✅ RESOLVED HERE — [`2026-11-18-coder-start-visibility-gaps.md`](../todo/2026-11-18-coder-start-visibility-gaps.md)

## Affected Packages

- **`tddy-remote-shell`**
  - `RemoteShellError::TimedOut` gains the command, a truncated head of it.
  - A recording decorator over `dyn RemoteShell` (`RecordedShell`) that reports each call to a sink.
- **`tddy-tool-engine`**
  - `start_ledger.rs`: `StartLedgerRecord` gains `remote_calls` (bounded), `branch`/`worktree` set from a new progress event, and `updated_at` per call.
  - The `GetSessionStart` and `DescribeSessionStartOptions` descriptions.
- **`tddy-agent-launch`**
  - `cli_session_worktree.rs` wraps the start's shell in the decorator and reports the cut worktree and branch.
  - `conversation_start.rs` is touched as needed.
- **`tddy-service`** (only if events cross the proto): `session.proto` `StartSessionEvent` gains `remote_call` and `worktree_ready`, with the committed TS regenerated.
- **`tddy-daemon-rpc`**: wiring, and `tests/async_start_acceptance.rs`.

## Scope

- [ ] Remote-call log on the start record
- [ ] Timeout error names the command
- [ ] Branch and worktree recorded at the cut, kept on failure
- [ ] `usable` described
- [ ] Each pinned by a failing test first
- [ ] Scoped gates

## Responsibility

What a start records about its remote work, and what its errors say.

## Boundaries

- No change to how a start runs: same commands, same order, same deadlines.
- No cleanup of what failed starts leave behind.
- No change to `usable`'s computation.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/coder-start/coder-transport` | the remote bound in `CoderShell::exec` | `TimedOut` is raised there; this node adds the command to it | change the bound |
| `feature/coder-start/target-agent-start` | the target home probe and the trust write as start-time one-shots | they show up in the log like every other call | change them |

## Draft PR contract

Commit 2 publishes:
- the `RemoteCallRecord` type and the `remote_calls` field;
- the `TimedOut { command }` field;
- the progress event carrying the worktree and branch;
- the decorator type, with stub bodies;
- failing tests in `tddy-remote-shell` (the timeout names its command; the decorator reports each call), `tddy-tool-engine` (ledger folding), and `tddy-daemon-rpc/tests/async_start_acceptance.rs` (end-to-end over a stand-in target: a failing fetch, branch and worktree kept, no token in the log).

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — its timeout test runs through `coder-transport`'s bounded `exec`, and its log must show `target-agent-start`'s new probe calls.
**Concurrent with:** none
**Blocks:** `target-test-kit`

## Technical Changes

### State A

Discovery, Exploration 3 § Start visibility. In short:
- `StartLedgerRecord` has no list.
- Only `Ending::Succeeded` carries branch and worktree.
- `TimedOut {address, after}` drops the command.
- `usable` is a status and CLI check.

### State B

The start's shell is a `RecordedShell` that reports each call to the start's progress sink. The
ledger folds the reports into `remote_calls` (capped, oldest dropped, credentials scrubbed) and
the worktree event into `branch`/`worktree`. `TimedOut`'s Display is
`"{address}: `{command}` gave no answer within {after:?}; the command was stopped"`.

## Decisions & Trade-offs

- **The decorator sits at the `RemoteShell` trait**, the one choke point every start-time call goes
  through (Exploration 3).
- **Reports reach the ledger as `StartSessionEvent`s (proto route, settled in commit 2).** The ledger
  drains a stream that, for a start on another host, is a relay of that daemon's events
  (`AttachmentProgressSink::relay`), while the start's shell lives on the daemon running the start.
  A ledger-owned in-process sink could not see a cross-host start's calls. `session.proto` gains
  `StartRemoteCall remote_call = 7` and `StartWorktreeReady worktree_ready = 8`; the committed TS is
  regenerated with `scripts/generated-code.sh write`.
- **`RecordedShell` (tddy-remote-shell) reports to a `RemoteCallSink`.** It records the caller's
  `request.command`, never a transport's wrapper (Coder's one-shot preamble). Credentials are
  scrubbed twice: in `RecordedShell`, so nothing secret crosses the proto, and again in the ledger
  on record. The ledger keeps at most `MAX_REMOTE_CALLS` (50), newest.
- **Credentials in the start's `error` (green, orchestrator, under the user's finish-without-stopping
  instruction):** `RemoteCheckoutError::Clone` Display carries the project's `git_url`, so the remote
  worktree step scrubs the error text (`tddy_remote_shell::scrub_credentials`) before it becomes the
  start's `Status`. The scrubber (user-info of `scheme://user:pw@host`) and the command/stderr
  truncation live in `tddy-remote-shell` (`redact.rs`) and are shared by the shell layer, the
  timeout error and the ledger.
- **Where the shell is decorated (green):** at the remote cut in `cli_session_worktree.rs`
  (`recorded_by`), so every call of the worktree step is logged; `worktree_ready` is reported when
  `setup_remote_worktree` returns, i.e. right after `git worktree add` (or the already-cut check)
  succeeded, which leaves `setup_remote_worktree`'s signature unchanged. Calls made by later steps
  (home probe, trust write) are not yet routed through the decorator.

## TODO

- [x] Record initial discovery
- [x] Create PRD
- [x] Create changeset
- [x] Draft PR contract — failing tests (commit 2)
- [ ] USER REVIEW — acceptance tests (6e)
- [x] TDD Green
- [ ] Validate changes
- [ ] Wrap
