# 2026-11-17 — Sessions send each other messages, read as activity

**Type:** Feature · `#activity-wake` 5/5, PR [#935](https://github.com/uppin/tddy-coder/pull/935), base `feature/activity-wake/watch-monitor` (#934)

Any session can send another session of its project a message it reads when it next looks — never
typed into its terminal. `PostSessionMessage { to_session_id, text, reply_to? }` is offered to
**every** session role; the message is recorded once, as a `message` record of the project
notification log (a new `SessionNotificationSource::Message` carrying a `SessionMessage`
`{ to_session_id, message_id, reply_to, text }`, published as `AttentionRequired` so the waker and
the long polls see it). The recipient reads it two ways, both references into the log, nothing of
its own kept: its **inbox watch** — `CreateSessionWatch` with the kind `message` alone and no
session selected, the one watch a session that is not an orchestrator may hold, delivered like any
watch and streamed by `tddy-tools monitor --kinds message` as `message` lines — or
**`ReadSessionMessages`**, which reads through the session's **inbox position** (a `seq` beside the
watches in `session-watches.json`) and advances past what it answers. A message is delivered once
across the reader's watches and its reads, under the project's lock. Who may address whom is the
daemon's rule, read from the sessions' own records: one project, one OS user, one host, never
oneself — an orchestrator addresses any session it can see, a session its orchestrator or the
project's main session, and a reply the sender of a message it received. Each refusal names the
reaches. Text is non-empty and at most 8 KiB; a read answers at most 50.

## What changed

- **tddy-daemon-kernel**: `SessionNotificationSource::Message` and `SessionMessage`, the `message`
  part on `SessionNotification`.
- **tddy-session-activity**: new `session_messages` — `WatchFilter::inbox_of`/`is_inbox_of`,
  `InboxMessage`, `InboxRead`, `refuse_a_bad_message_text`, `MAX_MESSAGE_BYTES` (8 KiB),
  `MAX_MESSAGES_PER_READ` (50), and `SessionWatchStore::read_messages`/`inbox_position`/
  `received_message` over the log; the `message` kind in the matcher (`WatchKind::ALL` is seven,
  still the default when no kind is named), the inbox watch's match and the once-only rule
  (`watch_matches` takes the owner's inbox position; `a_watch_has_delivered`), the inbox positions
  in `ProjectWatches` (`inbox_positions`); the `message` DTO in the log's storage; the web stream
  and `ReadProjectNotifications`/`WaitProjectNotifications` skip the source.
- **tddy-tool-engine**: new `session_message_tools` — the two tools' definitions (offered to every
  role in every catalogue), `SessionMessagePort`, `Addressable`, `MessageRefusal`,
  `execute_session_message_tool`; `WATCH_KINDS` gains `message`; `read_only.rs` admits them for
  the project-home role.
- **tddy-daemon-rpc**: new `exec_tool/session_messages.rs` — the port on the host
  (`DaemonSessionMessages`), the addressing rule read from the sessions' records, and the inbox
  watch for a session that is not an orchestrator (`run_inbox_watch_tool`, routed ahead of the
  project-home role gate, creating only the inbox watch and serving the other watch tools over the
  caller's own watches); `ProjectHomeHost` carries the bus.
- **tddy-session-lifecycle**: `DaemonSessionHost::session_notification_bus()`, the getter the
  daemon publishes a posted message through.
- **tddy-telegram-control, tddy-desktop**: one skip arm each — a message never reaches a chat or a
  native notification.
- **tddy-tools / tddy-tools-cli**: new `send_cli.rs` (`tddy-tools send --to <session-id>
  [--reply-to <id>] [<text> | -]`, one JSON answer line, non-zero with the reason when refused);
  `MonitorLine::Message` and `message_line_of` — a streamed event of kind `message` prints as a
  `message` line carrying the text whole (JSON escapes its newlines); the two tools registered for
  the MCP server (`exec_tool_admission.rs`).

## Verification

Scoped on head `8a74b05ea` + the wrap commit (base `origin/feature/activity-wake/watch-monitor`,
leak check clean — the range is this PR's commits only), `CARGO_INCREMENTAL=0
CARGO_PROFILE_DEV_DEBUG=0`: `cargo clippy` `-D warnings --all-targets` clean for
tddy-session-activity, tddy-tool-engine, tddy-daemon-rpc, tddy-tools, tddy-tools-cli,
tddy-telegram-control, tddy-daemon-kernel, tddy-desktop, tddy-session-tool-client and
tddy-session-lifecycle (the test-target dead-code failure in its `project_home_support` named at
green did not reproduce under this feature set — the whole batch ran `--all-targets` clean);
`cargo fmt --check` clean; scoped tests green one target at a time — 29 touched targets, 240
tests, 0 failures, among them `session_messages_acceptance` 10/10, `session_messages_unit` 7/7,
`send_and_message_monitor_cli_acceptance` 3/3 (the real monitor loop), `session_message_tools`
3/3, `monitor_message_line` 2/2, the six neighbouring `project_home_*` suites,
`operator_channel_acceptance` 14, `session_notification_bus_unit` 19, the six telegram suites and
the desktop pair (8+25). Whole-workspace health is CI's.

## File length

- 🔴 `packages/tddy-daemon-rpc/src/exec_tool/session_messages.rs` 0 → 619 production lines: a new
  file over budget. **Split in this PR**: the inbox-watch service for a session that is not an
  orchestrator moved to `exec_tool/session_messages/inbox_watch.rs` — the split is a byte-for-byte
  move of the one contiguous inherent-`impl` block into a child module, no moved code rewritten;
  619 → 307 (`session_messages.rs`, the message port) + 335 (`inbox_watch.rs`), each under budget.
  Route note: `/pr-wrap` step 3.5 routes a Rust split through the `code-restructuring` engine; the
  engine's anchor resolution did not return (the index daemon's warm-up did not finish in the
  session's time), so the move was done by hand under the same discipline — mechanical move only,
  imports updated, `check`/`clippy`/`fmt` and the scoped tests green after.
- 🔴 `packages/tddy-session-activity/src/session_watches.rs` 723 → 857 production lines: already
  over, grew here. **Not split**: stack-shared — #930, #931 and #934 also modify this file, so the
  split is deferred to a follow-up branch after the `#activity-wake` stack lands, under
  `/pr-wrap` step 3.5's stack-branch rule. Recorded in
  `packages/tddy-session-activity/docs/code-issues/oversized-file-session-watches.md`.
- 🔴 `packages/tddy-session-activity/src/project_notification_log/storage.rs` 536 → 578 production
  lines: already over, grew here. **Not split**: stack-shared — #929 and #930 also modify this
  file. Recorded in
  `packages/tddy-session-activity/docs/code-issues/oversized-file-storage.md`.

## Code issues

- No code-issue record is added by this PR: the new files end under budget (after the split
  above), and no open record in a touched package is claimed by #935. The two oversized-file
  records above were re-measured and carry the new rows. The changeset's three ⚠ DURING records
  were honoured, re-measured unchanged: `answer_execute_tool`
  (`complexity-exec-tool-ports-execute-tool.md`) gained no line — the message tools are served
  through the executor's existing dispatch; `run_call_tool` (`complexity-cli-run-call-tool.md`) is
  untouched — `send_cli.rs` is a module of its own calling the session-tool dispatch directly; the
  Telegram notifier (`complexity-telegram-notifier-on-server-message.md`) gained no line — a
  message never reaches it (the skip is one arm in the subscriber's source match).

## Backlog

No `docs/dev/todo/` entry is resolved here. The changeset's ℹ ANSWERED entry stays open as
recorded: a message needs no `turn_input`, so `PostSessionMessage` is the first way to tell an ACP
or Cursor session anything
(`2026-11-13-acp-turn-input-is-not-delivered.md`) — the entry's reason now has its answer, and the
entry stays for the turn it still cannot deliver. The stack's backlog delta
(`2026-11-16-session-watches-are-not-shown-on-the-web.md`, added by the stack) is the sweep
question, not this node's.

## Deferred out of this node

- **Sibling addressing** (PRD open question 1, decided by the user 2026-11-17): a session may not
  message a sibling — another session its orchestrator started; the orchestrator relays, which
  keeps it the one session that knows the conversation. Siblings would be one more addressing rule.
- **Tool-call activity as watchable activity** (user, 2026-11-17): the `agent-activity.jsonl` rows
  a session's tools write are activity too, and a watch could filter on them as it filters on
  `message`.
- **A log of messages' own** if a chatty pair of agents evicts human-facing notifications (PRD
  open question 2, accepted for now): messages share the log's 1 000-record window.
