# Initial discovery — watch-monitor (`#activity-wake` 4/5)

**Changeset:** [`2026-11-17-watch-monitor`](./2026-11-17-watch-monitor.md) · **Base:** `feature/activity-wake/idle-wake` (#931) · nothing built or run.

Exploration 1 is the whole stack-extension's discovery (nodes 4/5 and 5/5), copied in full; node-specific follow-up is appended as later explorations.

## Exploration 1 (whole extension) — combined conclusions

1. **The daemon serves no WebSocket.** Its HTTP listener serves Connect RPC (`/rpc`) and the web
   bundle; the only `ws://` / `wss://` in the tree are LiveKit URLs
   (`packages/tddy-daemon/src/daemon_settings.rs:15`, `LIVEKIT_URL_SCHEMES`). Claude Code's
   `Monitor` `ws` mode is a WebSocket *client* (`{url, protocols}` only, no headers), so a
   daemon-side WebSocket *client* to an agent-chosen URL cannot reach it without a relay. Decision:
   no WebSocket in this stack; the agent stream is a command (`tddy-tools monitor`) under
   `Monitor({ command })`.
2. **`tddy-tools` already reaches the daemon over every session transport**
   (`tddy-session-tool-client`: `dispatch_via_sandbox_ipc`, `dispatch_via_daemon_uds`,
   `dispatch_via_daemon_http`, `dispatch_via_rpc_transport`, `dispatch_via_streaming_rpc`;
   selection in `detect_session_tool_transport`, `lib.rs`). A monitor that loops a blocking tool
   call inherits all of them, jail and split included.
3. **Blocking tools are clamped for remote calls** (`clamp_remote_blocking_args`,
   `remote_block_arg_keys` in `tddy-session-tool-client/src/lib.rs`: `WaitProjectNotifications` →
   `timeout_ms`). A new long-poll tool must be added there.
4. **`WaitProjectNotifications` is the long-poll model**: `MAX_WAIT_MS = 20_000`
   (`tddy-tool-engine/src/project_home_inbox_tools.rs:41`), holds until the project's sessions raise
   something or the timeout passes.
5. **The watch store (#930/#931) already has the two delivery modes a stream needs to sit beside**
   (`tddy-session-activity/src/session_watches.rs` on `idle-wake`):
   - `take_delivery(owner, log, now)` — advance-and-persist under one per-project lock;
   - `deliver_with(owner, log, now, deliver)` — advance only if `deliver` succeeds (#931, the wake);
   - `has_live_watches(owner, log, now)` — the auto-stop hold (#931).
   A streamed watch needs a third: advance on acknowledgement, and the first two must skip it while
   a stream holds it.
6. **The wake is one module** (`tddy-daemon-rpc/src/exec_tool/watch_wake.rs`): `WakeTrigger` (bus
   subscriber, cue and return), `WatchWaker` (settle 2 s / gap 30 s, `WakeTimings`),
   `orchestrator_is_idle`, `wake_text` (the per-event wording a monitor line should reuse). The
   stream lease check belongs where the waker takes its delivery.
7. **The `watchEvents` attachment is one function** around `execute_tool` (#930,
   `exec_tool/watch_events.rs`); a streamed watch must be skipped there too — via the store, so the
   attachment itself needs no change if `take_delivery` skips streamed watches.
8. **`SendSessionMessage` is the only agent-to-agent verb today**
   (`tddy-tool-engine/src/project_home_turn_tools.rs:35`): orchestrator-only, pastes a turn, refused
   for an agent without `turn_input` (`tddy-daemon-rpc/src/exec_tool/project_home/turn.rs:118`).
   Cursor declares none (`docs/dev/todo/2026-11-16-cursor-hooks-udoo.md`). There is no mailbox.
9. **Watches are orchestrator-only** (the project-home tool role,
   `SessionToolRole::ProjectHome`); visibility is `ProjectNotificationScope { project_id, os_user,
   exclude_session_id, started_by }`, one host.
10. **`tddy-tools` subcommands live in `packages/tddy-tools-cli/src/main.rs`** (clap `Subcommand`,
    e.g. `Notify(notify_cli::NotifyArgs)`, `RenameSession`, `CallTool`); session-scoped commands find
    their session through `tddy_tools::session_source::resolve_current_session`.

## Exploration 1a — this conversation's reading (2026-11-17)

**Sequence**

1. `git fetch origin feature/activity-wake/{session-ended-notice,session-watches,idle-wake}`;
   `git log origin/master..origin/feature/activity-wake/idle-wake` — 9 commits, 3 nodes, each with
   plan / contract / green commits.
2. Read `docs/dev/1-WIP/2026-11-16-session-watches.md`,
   `docs/ft/daemon/1-WIP/PRD-2026-11-16-session-watches.md`,
   `docs/ft/daemon/1-WIP/PRD-2026-11-16-idle-wake.md` on `idle-wake` — the watch model, six kinds,
   `watchEvents`, exactly-once, endings, the wake, debounce, auto-stop hold.
3. `grep -rln -i "websocket|WebSocketUpgrade|tokio_tungstenite|axum::extract::ws" packages` — hits
   are LiveKit, screenshare bridge, codex stream, testkits; none is a daemon WebSocket server.
   `daemon_settings.rs:15,132` — `ws://`/`wss://` only for `livekit.url`.
4. `grep "pub async fn|pub fn" packages/tddy-session-tool-client/src/*.rs` — the transports listed in
   conclusion 2; `rpc_transports.rs:255` `dispatch_via_streaming_rpc` (`StreamExecuteTool`, server
   stream, used for split sessions over LiveKit; its await is unbounded — `TODO(livekit-rpc-deadline)`).
5. `lib.rs` (`clamp_remote_block_ms`, `remote_block_arg_keys` → `"WaitProjectNotifications" =>
   &["timeout_ms"]`).
6. `tddy-tools-cli/src/main.rs:39-121` — the `Subcommand` enum.
7. `session_watches.rs:385-460` on `idle-wake` — `take_delivery`, `deliver_with`, `has_live_watches`
   (excerpts in conclusion 5).
8. `watch_wake.rs:1-110` on `idle-wake` — module doc, `WAKE_CUE_CAPACITY = 1_024`, `WakeTimings`
   (`settle: 2 s`, `min_gap: 30 s`), `WakeCue`, `WakeTrigger`, `orchestrator_is_idle` (line 106).
9. `project_home_inbox_tools.rs` — `WAIT_PROJECT_NOTIFICATIONS`, `MAX_WAIT_MS = 20_000`.
10. `project_home_turn_tools.rs:80-100` — `SendSessionMessage`'s definition: `session_id`, `text`,
    `target_daemon_instance_id`; "Give a running session of this project a new turn…".
11. Claude Code `Monitor` tool schema (loaded in-session): `command` | `ws {url, protocols}`,
    `description`, `timeout_ms` (default 300 000, capped at 1 800 000); stdout lines → notifications,
    200 ms batching; `TaskStop` cancels.

**Unknowns carried into the plan**

- Whether a `Monitor` notification starts a turn for an *idle* Claude Code agent (decides whether a
  stream may suppress the wake). Recorded as open question 1 in the watch-monitor PRD; needs a manual
  check before green.

## Exploration 1b — Step 2b cross-check and the surfaces each node touches

**Sequence**

1. `git ls-tree` of `packages/<pkg>/docs/code-issues` on `idle-wake` for every package either node
   touches; `git grep -l` over `docs/dev/todo` for the area's words. Relevant hits below; the rest
   (Telegram handler complexity, web oversized files, kernel privilege-drop tests) are in packages
   these nodes touch only by a one-arm `match` addition, or not at all.
2. `git show idle-wake:packages/tddy-daemon-kernel/src/session_notification.rs:20-60` —
   `SessionNotificationSource { ActivityStatus, AgentToolCall, Presenter, Agent, PendingQuestion,
   SessionEnded }`; `SessionEnded` carries a `SessionEndNotice` (`#activity-wake` 1/3).
3. `git grep "SessionNotificationSource::"` on `idle-wake` (non-test) — every place a new source
   must be handled: `tddy-session-activity/src/streams.rs` (9, the web stream mapping),
   `session_watches.rs` (7, the matcher), `tddy-telegram-control/src/telegram_session_subscriber.rs`
   (6), `project_notification_log/storage.rs` (6), `session_notifications.rs` (4),
   `tddy-session-lifecycle/src/session_notifications.rs` (3), `exec_tool/watch_wake.rs` (3),
   `service/operator_notifications.rs`, `presenter_observer_task.rs`,
   `tddy-desktop/src-tauri/src/native_notifications.rs` (1 each).
4. `git grep SESSION_ENDED` in `tddy-session-web` — `src/lib/sessionNotificationDot.ts:23`: the drawer
   ignores `SESSION_ENDED` for its attention dot; the source is on the stream proto. A `message`
   source must instead never reach the web stream or Telegram (skipped in `streams.rs` and
   `telegram_session_subscriber.rs`), so no proto change is needed.
5. `git grep orchestrator_session_id` — `tddy-agent-launch/src/conversation_spawn.rs:88`,
   `hooks_and_urls.rs:90`: the session record names the orchestrator that started it — the addressing
   rule of session messages reads it.

**Step 2b verdicts**

| Entry | Node | Verdict |
|---|---|---|
| [`complexity-exec-tool-ports-execute-tool.md`](../../../packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-execute-tool.md) (now names `answer_execute_tool`) | 4, 5 | ⚠ DURING — new tools route through the executor's existing dispatch; not one line added to `answer_execute_tool` |
| [`complexity-cli-run-call-tool.md`](../../../packages/tddy-tools/docs/code-issues/complexity-cli-run-call-tool.md) | 4, 5 | ⚠ DURING — `monitor` / `send` are modules of their own (`monitor_cli.rs`, `send_cli.rs`, as `notify_cli.rs`), calling the dispatch, never growing `run_call_tool` |
| [`oversized-file-server.md`](../../../packages/tddy-tools/docs/code-issues/oversized-file-server.md), [`2026-10-08-tddy-tools-server-rs-is-3-9k-lines…`](../todo/2026-10-08-tddy-tools-server-rs-is-3-9k-lines-and-the-crate-mixes-several-responsibilities.md) | 4, 5 | ℹ ANSWERED — nothing lands in `server.rs` |
| [`2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md`](../todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md) | 4 | ⚠ DURING — a split session's monitor long-polls over LiveKit; the monitor bounds each call itself (`timeout_ms` + margin) and exits non-zero on a lapse, rather than hang silently under `Monitor` |
| [`2026-11-13-acp-turn-input-is-not-delivered.md`](../todo/2026-11-13-acp-turn-input-is-not-delivered.md) | 5 | ℹ ANSWERED — a message needs no `turn_input`; the entry stays open |
| [`2026-11-16-session-watches-are-not-shown-on-the-web.md`](../todo/2026-11-16-session-watches-are-not-shown-on-the-web.md) | 4 | ℹ unrelated here — the monitor adds no web surface |
| Telegram notifier code issues (`complexity-telegram-notifier-on-server-message.md` et al.) | 5 | ⚠ DURING — the `message` skip goes in `telegram_session_subscriber.rs`'s source match, one arm, nothing in the notifier |

## Exploration 2 — pass 2 red (contract and failing tests), 2026-11-17

**Sequence**

1. `session_watches.rs` on this branch: `take_delivery` and `deliver_with` share one private
   `apply_delivery(state, owner, log, now)`, so the stream's skip is one place (marked
   `TODO(watch-monitor)` there). The store's two `Self { .. }` literals in `open` gained the lease map
   and the grace.
2. `exec_tool/project_home_watches.rs` and `watch_events.rs` read the clock as
   `tddy_daemon_kernel::now_unix_ms()`, not an injected one: an acceptance test cannot move time, so
   a lease lapse at that level needs the grace itself to be shorter — hence
   `SessionWatchStore::with_stream_lease_grace`, set by the new support helper
   `a_host_streaming_watches` (beside `a_host_waking_orchestrators`, which now shares its builder).
3. `watch_wake.rs`'s `owners_concerned` asks `has_live_watches`, which a streamed watch still
   answers yes; `try_wake` then gets `Ok(false)` from `deliver_with` once it skips the watch — so the
   waker needs no edit, as the Boundaries say.
4. `project_home.rs` `run_locally` routes only names `is_project_home_tool` lists, so the new tool is
   listed there (and offered in the catalogue) for a call to reach `DaemonProjectHome::execute`.
5. Catalogue fallout: three name lists pin the project-home catalogue exactly —
   `read_only.rs`'s unit test, `project_home_session_tools_acceptance.rs`, and #930's two
   `contains("SessionWatch")` filters (`tests/project_home_watch_tools.rs`,
   `project_home_session_watch_acceptance.rs`), which `WaitSessionWatchEvents` also matches. All four
   now include it.
6. `tddy-tools-cli` has no dependency on `tddy-daemon-rpc`, and the daemon harness is a test
   module there; the CLI suite instead serves a scripted `ExecToolService` on the session's
   `TDDY_SANDBOX_TOOL_IPC` socket, as `notify_cli_acceptance.rs` and `support/exec_catalog.rs` do.

**Open point — resolved**

- **Re-arming inside the lease.** `Monitor` expires after 30 minutes and the agent re-arms with
  `--watch`; the new process presents no `stream_id`, so as first drafted it was refused as a second
  stream until the old lease lapsed (up to 15 s). **Resolved by the user (2026-11-17): resume by
  takeover** — a new stream of the same owner takes the watch over at once, resuming from the last
  acknowledged position, and the superseded stream is refused from its next call
  (`StreamRefusal::Superseded`, replacing the draft's `StreamedElsewhere`). An unresumed watch ends
  at its TTL. Tests: `a_new_stream_on_a_streamed_watch_takes_it_over_and_the_old_stream_is_refused`,
  `a_new_stream_takes_over_from_the_old_one`,
  `monitor_with_watch_resumes_a_watch_another_monitor_was_streaming`.
