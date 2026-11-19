# 2026-11-18 — A failing start on a target says which command failed, and what it left behind

**Type:** Feature · `#coder-start` 8/10, PR [#948](https://github.com/uppin/tddy-coder/pull/948)
**Packages:** `tddy-remote-shell`, `tddy-service`, `tddy-session-files`, `tddy-agent-launch`, `tddy-tool-engine`, `tddy-daemon-rpc` (tests)

The Coder `remote-dev` smoke test of 2026-11-18 was diagnosed only by sampling `ps` on both hosts:
the daemon recorded no command, no exit code and no stderr for a start, a timeout did not say what
hung, and every failed start reported `branch: null` / `worktree: null` while each had cut both.

- **Every call a start makes on its target is recorded.** `tddy-remote-shell`'s `RecordedShell`
  decorates `dyn RemoteShell` and tells a `RemoteCallSink` each `exec`'s address, the caller's
  command (never a transport's wrapper), duration, outcome (`Exited { code }`, `TimedOut`,
  `Unreachable`) and, for a failed call, its stderr tail. `tddy-agent-launch` wraps the start's
  target shell at the remote cut (`cli_session_worktree::recorded_by`) and again for every later
  step in `opencode_cli_spawn::start_cli_harness_session`. The sink holds the progress stream
  weakly, so a shell kept past the start never holds the stream open nor reports after it.
- **Reports cross the proto** as `StartSessionEvent.remote_call` (`StartRemoteCall`, field 7) and
  `worktree_ready` (`StartWorktreeReady`, field 8), because the start ledger drains a stream that,
  for a start on another host, is that host's relay; an in-process sink could not see those calls.
  The committed TS (`tddy-client-web`, `tddy-rust-typescript-tests`) is regenerated.
- **The start ledger folds them** into `GetSessionStart`'s `remote_calls` (oldest first, at most 50,
  newest kept; each call advances `updated_at_unix_ms`) and `branch`/`worktree`, which stay on the
  record when the start fails, is cancelled, or succeeds with no session listing to read them from.
  The log types live in `tddy-tool-engine/src/start_ledger/remote_calls.rs`.
- **Credentials never reach the record.** `tddy-remote-shell/src/redact.rs` (`scrub_credentials`,
  `command_head` at 200 characters, a 400-character stderr tail) is applied in the recorder, again in
  the ledger (so a relayed start from a daemon that did not scrub still leaves none), and to the
  remote clone error before it becomes the start's `Status`.
- **A timeout names its command**: `RemoteShellError::TimedOut { address, after, command }`, shown as
  `` <address>: `<command>` gave no answer within <after>; the command was stopped ``.
- **`usable` is described** in `DescribeSessionStartOptions`: *listed as startable*, never a probe.

**Validation fixes** (orchestrator, under the user's finish-without-stopping instruction): the
post-cut target calls were added to the log; the sink was made weak so a kept shell cannot pin the
start's stream; and a succeeded start with no session listing no longer erases the branch/worktree it
recorded (pinned red→green by `a_succeeded_start_whose_session_is_not_listed_keeps_the_branch_and_worktree_it_cut`).
`start_ledger.rs` reached 531 production lines and was split by the restructure engine (`move_item`)
to 485 + 54; `restructure verify` against the pre-split commit lost no statement. The 6e
acceptance-test review was pre-approved under the same instruction.

**Pinned by** `async_start_acceptance`'s `a_start_whose_fetch_fails_reports_the_failing_call_and_names_its_step`
and `no_credential_reaches_the_remote_call_log` (end to end over a stand-in SSH target), the
`recorded_shell` and `a_one_shot_past_its_deadline_names_its_command` tests in `tddy-remote-shell`,
the ledger-folding tests in `tddy-tool-engine/tests/start_ledger.rs`, and
`recorded_start_shell_tests` in `tddy-agent-launch`.

**Gates (scoped, on `90a18a776` + docs):** `./test -p tddy-tool-engine` 284 passed / 0 failed (38
suites); `./test -p tddy-agent-launch -p tddy-remote-shell` 426 passed / 0 failed (28 suites);
`cargo test -p tddy-daemon-rpc --test async_start_acceptance` 9 passed / 0 failed; `cargo clippy
--all-targets -D warnings` clean on tool-engine, agent-launch, remote-shell and session-files.
Whole-workspace health is CI's.

**Code issues.** `complexity-opencode-cli-spawn-start-cli-harness-session` (`tddy-agent-launch`):
**worse by 6** (391 → 397, brace-matched on `origin/feature/coder-start/target-agent-start`
`a91b974ec` and HEAD) — the `recorded_by` wrap of the target shell; not extracted, restructuring
stays with the `#agent-tools` sweep as deferred.

**Backlog resolved:** *2026-11-18 — A failing session start on a target cannot be explained from what
the daemon records* (`2026-11-18-coder-start-visibility-gaps`). Final state against its "What would
close it": a per-start log of the remote calls (host, command, duration, exit or timeout, stderr
tail) readable through `GetSessionStart` ✅; branch and worktree recorded at the cut ✅; the timeout
names its command and the start's `error` names its step ✅; `usable` documented ✅. The log records
the command, not a separate argv or cwd: a target command is one shell string whose `cd` is part of it.

**Out of scope:** a probe-based `usable`, and cleaning up what failed starts leave behind.

**Docs:** [project-concept.md § Project-home tools](../../ft/daemon/project-concept.md),
[tddy-remote-shell architecture.md § Recording calls](../../../packages/tddy-remote-shell/docs/architecture.md),
[tddy-service start-session-phases.md](../../../packages/tddy-service/docs/start-session-phases.md),
[tddy-session-files host-documents-and-attachments.md](../../../packages/tddy-session-files/docs/host-documents-and-attachments.md),
[tddy-agent-launch module-layout.md](../../../packages/tddy-agent-launch/docs/module-layout.md).
