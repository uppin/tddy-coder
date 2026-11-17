# 2026-11-17 — An orchestrator streams its watches to a native monitor

**Type:** Feature · `#activity-wake` 4/5, PR [#934](https://github.com/uppin/tddy-coder/pull/934), base `feature/activity-wake/idle-wake` (#931)

`tddy-tools monitor` streams an orchestrator's watch events as one compact JSON object per stdout
line, for the agent's own monitor tool (Claude Code's `Monitor`) to turn into notifications —
mid-turn or idle, with no paste and no busy/idle guess. It creates a watch (or attaches with
`--watch`) and loops a new project-home tool, `WaitSessionWatchEvents`, which acknowledges what was
printed and answers the next batch, held on the notification bus like `WaitProjectNotifications`
(the same 20 s cap, the same remote clamp for `timeout_ms`). While a watch is streamed, and for
15 s after each call, its events go to the monitor alone; when the lease lapses, everything after
the last acknowledged `seq` returns to tool answers and the wake. A killed monitor loses nothing:
a batch received but not acknowledged is handed again, and `seq` marks the possible duplicate. A
new monitor on the watch's own orchestrator takes the stream over at once; the superseded stream is
refused from its next call. The monitor exits 0 once every streamed watch has ended and non-zero
after an `error` line on any refusal, transport loss or per-call deadline lapse (`timeout_ms` +
10 s).

## What changed

- **tddy-session-activity**: `StreamBatch`, `StreamRefusal`, `STREAM_LEASE_GRACE` (15 s),
  `SessionWatchStore::take_stream_batch`, `is_streamed` and `with_stream_lease_grace` — the
  stream lease and the acknowledgement in a new `session_watches/stream.rs`; the skip of a
  streamed watch inside the shared selection `take_delivery` and `deliver_with` both use
  (`apply_delivery`).
- **tddy-tool-engine**: new `project_home_watch_stream_tools` — `WAIT_SESSION_WATCH_EVENTS`, its
  definition, refusal texts and `ProjectHomeWatchStreamPort`; the project-home catalogue lists it,
  and `read_only.rs`/`project_home_tools.rs` carry it through the shared tool plumbing.
- **tddy-daemon-rpc**: the port on `DaemonProjectHome`
  (`exec_tool/project_home_watch_stream.rs`) — the bus-woken long poll whose every re-take renews
  the lease, answering each event with the monitor line's fields plus its log record;
  `exec_tool/project_home.rs` routes the tool.
- **tddy-session-tool-client**: `WaitSessionWatchEvents`'s `timeout_ms` in the remote
  blocking-argument clamp.
- **tddy-tools / tddy-tools-cli**: new `monitor_cli.rs` (`MonitorArgs`, `MonitorLine`, the loop,
  the line protocol, exit codes) and the `Monitor` subcommand; `exec_tool_admission.rs` admits the
  new tool.

## Verification

Scoped on head `5033bfbeb` + the wrap commit (base `origin/feature/activity-wake/idle-wake`, leak
check clean — the range is this PR's commits only), `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`:
`cargo clippy -p tddy-session-activity -p tddy-tool-engine -p tddy-daemon-rpc -p tddy-tools -p
tddy-session-tool-client -p tddy-tools-cli --all-targets -- -D warnings` clean; `cargo fmt
--check` clean; scoped tests green one target at a time — `session_watch_stream_unit` 6/6,
`project_home_watch_stream_tools` 3/3, `wait_session_watch_events_clamp` 1/1,
`project_home_watch_stream_acceptance` 8/8, `monitor_cli_acceptance` 6/6, and the neighbouring
`project_home_session_watch_acceptance` 13/13 and `project_home_watch_tools` 5/5. Whole-workspace
health is CI's.

## File length

- 🔴 `packages/tddy-session-activity/src/session_watches.rs` 701 → 723 production lines: already
  over, grew here. **Not split**: the successor PR #935 (`feature/activity-wake/session-messages`)
  also modifies this file, so the split is deferred to a follow-up branch after the
  `#activity-wake` stack lands, under `/pr-wrap` step 3.5's stack-branch rule. Recorded in
  `packages/tddy-session-activity/docs/code-issues/oversized-file-session-watches.md`.
- New files all under budget: `session_watches/stream.rs` 423, `monitor_cli.rs` 291,
  `project_home_watch_stream_tools.rs` 197, `project_home_watch_stream.rs` 138 production lines.

## Code issues

- No code-issue record is added by this PR and none in a touched package is claimed by it. The
  oversized-file record above was re-measured (723, still open, deferred per the stack rule).
  Pre-existing records on this PR's other surfaces were re-measured unchanged: `answer_execute_tool`
  (`complexity-exec-tool-ports-execute-tool.md`) gained no line — the stream tool is served through
  the project-home executor's existing dispatch; `run_call_tool`
  (`complexity-cli-run-call-tool.md`) is untouched — `monitor_cli.rs` is a module of its own
  calling the session-tool dispatch directly; nothing landed in `server.rs`
  (`oversized-file-server.md`). The open records stay as they are.

## Backlog

No `docs/dev/todo/` entry is resolved here. The changeset's three ⚠ DURING entries stay open as
recorded: the two complexity records above, and the client-side LiveKit deadline
(`2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md`) — the monitor bounds every call
itself, which bounds one caller, not the transport. The stack's backlog delta
(`2026-11-16-session-watches-are-not-shown-on-the-web.md`, added by the stack) is the top node's
sweep question, not this node's.
