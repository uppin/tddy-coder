# 2026-11-17 — Orchestrators watch the sessions they can see

**Type:** Feature · `#activity-wake` 2/3, PR [#930](https://github.com/uppin/tddy-coder/pull/930), base `feature/activity-wake/session-ended-notice` (#929)

Orchestrators get `CreateSessionWatch` / `ListSessionWatches` / `RenewSessionWatch` /
`CancelSessionWatch`. A watch is a filter (sessions by id and/or "started by me", six kinds) with
its own position in the project notification log and a required TTL ≤ 24 h. Every tool answer an
orchestrator receives on its host carries, under `watchEvents`, the records its watches matched
since the last answer — each once — plus `lost` and the watches that `ended`. Watches persist per
project and end on expiry, cancel, the orchestrator's own end, or every named session's end. No
wake: an orchestrator that makes no tool call receives nothing; that is `#activity-wake` 3/3.

## What changed

- **tddy-daemon-kernel**: `SessionNotification.activity_status: Option<String>` — the wire status
  an activity-status notification was raised for, `None` on every other literal.
- **tddy-session-activity**: `notification_for_activity_status` sets `activity_status`; the project
  log's DTO keeps it (`"activity_status"`, absent on older lines) and gains the unscoped
  `records_after(project, os_user, seq)` reader plus `head_seq`. New `session_watches` module:
  `SessionWatchStore` (create, list, renew, cancel, `take_delivery`, `has_live_watches`), persisted
  per project as `<data dir>/projects/<project_id>/session-watches.json` — whole-file atomic
  rewrite on every change, one async lock per project so a record is delivered once across
  concurrent calls. The matcher (`WatchKind::of`) tells `turn_finished` (`Done`) from
  `waiting_for_input` (`WaitingForInput` with no question); a status record carrying a question
  matches nothing, so one prompt is never delivered twice. Limits: `MAX_WATCH_TTL_SECS` (86 400),
  `MAX_LIVE_WATCHES` (16), `MAX_EVENTS_PER_ANSWER` (50).
- **tddy-tool-engine**: `project_home_watch_tools` — the four tool definitions, `ProjectHomeWatchPort`,
  and every refusal's wording; the project-home catalogue offers them to the project-home role only,
  and the executor routes them to the port.
- **tddy-daemon-rpc**: `DaemonProjectHome` implements the port (the caller's agent must be one a
  turn can be sent to, worded as `SendSessionMessage` words it; every named session must be one the
  caller can see on this host). `ExecToolRpcHandler::execute_tool` now delegates to
  `answer_execute_tool` and passes its answer through `with_watch_events`, which takes a delivery
  only for a successful, object-shaped answer to a caller this host holds, and inserts
  `watchEvents` when the delivery is non-empty — so an error answer consumes nothing.
- **tddy-session-lifecycle**: `DaemonSessionHost::with_session_watches` / `session_watches`
  (`svc_session_watches_wiring.rs`).
- **tddy-daemon**: `open_shared` opens the store beside the notification log and hands it to the
  exec-tool handler.
- Every crate with a `SessionNotification` literal gains `activity_status: None` — the field's
  mechanical fallout, no behaviour change in those crates.

## Verification

Scoped on the wrapped head, `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`: `./test -p
tddy-daemon-kernel -p tddy-session-activity -p tddy-tool-engine -p tddy-daemon-rpc
-p tddy-session-lifecycle -p tddy-telegram-control` — 310 targets ok, 2561 tests passed; the 26
failures are all in 7 pre-existing environmental targets (the sandboxed-codebase seatbelt suites,
where a jailed session cannot start on this Mac — `/bin/cat: illegal option -- -` — the
`session_sync_livekit` mirror suite poisoned by the same start failure, and the documented macOS
`/tmp` canonicalisation assertion in `claude_cli_from_spec_acceptance`), each of which failed the
same way before the rebase. This PR's own suites are green: `session_watches_unit` 11/11,
`project_home_watch_tools` 5/5, `project_home_session_watch_acceptance` 13/13, and the
`activity_status` storage cases. `./test -p tddy-daemon` — 335 passed, 1 failed (a 10 s
terminal-recording timeout under shared-machine load, in untouched split-session code).
`cargo clippy` on all seven touched packages with `--all-targets -- -D warnings` and
`cargo fmt --check` clean. Whole-workspace health is CI's.

## File length

- 🔴 `packages/tddy-session-activity/src/session_watches.rs` 0 → 650 production lines: this PR
  pushed it past 500. **Not split**: successor PR #931 also modifies this file, so the split is
  deferred to a follow-up branch after the `#activity-wake` stack lands, under `/pr-wrap` step
  3.5's stack-branch rule. Recorded in
  `packages/tddy-session-activity/docs/code-issues/oversized-file-session-watches.md`.
- 🔴 `packages/tddy-daemon/src/runtime/mount_host.rs` 516 → 527: already over, grew here. **Not
  split**: predecessor #929 and successor #931 also modify this file. Recorded in
  `packages/tddy-daemon/docs/code-issues/oversized-file-mount-host.md`.
- 🔴 `packages/tddy-session-activity/src/project_notification_log/storage.rs` 530 → 536: already
  over, grew here. **Not split**: predecessor #929 also modifies this file. Recorded in
  `packages/tddy-session-activity/docs/code-issues/oversized-file-storage.md`.

## Code issues

- `packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-execute-tool.md` is
  **deleted**: the finding's symbol moved — `execute_tool` is now a 10-line wrapper and its former
  body is `answer_execute_tool` at **59 lines, under the 60 budget** (was 69). Final measurement
  recorded here, per the deletion rule.
- No code issue is claimed by this PR, and none in a touched package carries a claim. The
  changeset's ⚠ DURING entries stay open and did not grow: `build_session_host` re-measured
  unchanged at 82 lines (no line of it was added to here), and the `execute_tool` length issue
  above, now closed by the move. Re-measure it in its record,
  `packages/tddy-daemon/docs/code-issues/complexity-mount-host-build-session-host.md`.
- Pre-existing records on code this PR touched were re-measured (file length, above); the rest
  stay as they stood.

## Backlog

No `docs/dev/todo/` entry is resolved here. One entry was added by this node's planning and stays:
`2026-11-16-session-watches-are-not-shown-on-the-web.md` — a Future enhancement (the developer
chose no web view in this stack); it needs an RPC and a view of its own, so it is genuinely
deferred, not an extra node of this stack. The deferred file splits are code-issue records, not
TODO entries, per the stack rule. The changeset's ℹ ANSWERED entries stay open as recorded.
