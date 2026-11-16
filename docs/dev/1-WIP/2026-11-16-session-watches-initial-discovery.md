# Initial discovery — an orchestrator watches the sessions it can see (`#activity-wake` 2/3)

**Changeset:** [`2026-11-16-session-watches`](./2026-11-16-session-watches.md) · **Base:** `feature/activity-wake/session-ended-notice` (#929) on `master` @ `769c16e86` · nothing built or run.

Exploration 1 is the whole stack's discovery, copied in full; Exploration 3 is this node's own.

**Changeset:** `2026-11-16-orchestrator-activity-wake` · **Base:** `master` @ `72491669b` · nothing built or run.

## Exploration 1 (whole stack) — combined conclusions

1. **The record's `kind` and `source` are two fields.** `kind` is `activity` | `attention_required`;
   `source` is `activity_status` | `agent_tool_call` | `presenter` | `agent` | `pending_question`.
   The project notification log stores **only** `attention_required`. A watch filter is therefore
   best expressed over a new, small vocabulary derived from (kind, source, question.kind, status),
   not over the raw `kind`.
2. **Only claude-cli can be woken.** `claude.yaml` is the only spec declaring `turn_input`
   (`terminal_paste`); `acp_prompt` parses but the daemon refuses it. claude-cli is also the only
   agent with `project_home_main`, so every orchestrator today is wakeable — the refusal at watch
   creation is a guard for future agents, reusing `turn_input_of`'s wording.
3. **`SendSessionMessage` has no mid-turn check and the daemon has no queue.** It refuses only: no
   `turn_input`, not `is_active`, pending question. Idle-vs-busy for a wake must be decided by the
   wake path itself from the orchestrator's `activity_status`.
4. **No session-exit event exists on the bus or in the log.** `Ended` raises no notification, and a
   main PTY exit only logs and runs exit hooks. "failed/exited" as a watchable kind — and "auto-end
   when every watched session has ended" — both need a new publish site.
5. **`/clear` writes `activity_status: Ended`.** `Ended` in `.session.yaml` is not proof of exit; an
   orchestrator that just cleared is idle and alive. Idle = `Done` | `WaitingForInput`-without-a-
   pending-question | `Ended`/`Started` with a live terminal; busy = `Running` | `ExecutingTool`.
   The idle reaper treats only `Running` as busy — do not copy that rule.
6. **The daemon session id survives `/clear`**; only `agent_conversations` grows. A watch keyed by
   session id survives rotation for free.
7. **The bus publishes sequentially inside the hook RPC.** A wake subscriber must enqueue and
   return; delivery (a terminal paste) happens on its own task.
8. **Visibility is already one rule**: `ProjectNotificationScope { project_id, os_user,
   exclude_session_id, started_by }`, one host per call. Excluding the caller's own session is
   already part of it, which is the no-self-wake rule.
9. **Every tddy-tools call an orchestrator makes passes one function**
   (`ExecToolRpcHandler::execute_tool` → `run_locally` → `DaemonProjectHome::execute`), so
   "matched events ride on each tool response" has one seam — but `execute_tool` is already over
   its length budget (code issue, below).
10. **No watch/subscription concept and no DB to put one in.** Per-project state is files under
    `<data dir>/projects/<project_id>/`; `.session.yaml` is `deny_unknown_fields`.
11. **Doc/code disagreement** (not this change's to fix, recorded): the package doc says an
    unreadable changeset's record "is still kept"; the code drops it
    (`project_notification_log.rs:317-329`).
12. **Caller identity is `ExecuteToolRequest.session_id`, taken as given**; the token resolves only
    to an OS user. A watch's owner is therefore as trustworthy as every other project-home tool's
    caller — no weaker, no stronger.

## Exploration 1 — Explore agent, 2026-11-16

### 1. Project notifications

- Record: `packages/tddy-session-activity/src/project_notification_log.rs:70-90` — `seq, session_id,
  label, kind, source, text, agent: Option<AgentNotification>, question: Option<QuestionNotice>,
  at_unix_ms, orchestrator_session_id: Option<String>`.
- Enums: `packages/tddy-daemon-kernel/src/session_notification.rs:8-13` (kind), `:22-35` (source),
  `:60-71` (`AgentNotification { title, body_markdown, level, sound, link }`), `:80-90`
  (`QuestionNotice { request_id, kind, tool_name, summary, questions }`), `:109-139`
  (`SessionNotification`).
- JSON DTO (file line and tool answer share it): `project_notification_log/storage.rs:224-256`,
  snake_case; a persisted line also carries `os_user` (`:200-203`).
- Append: `ProjectNotificationLog` is a `SessionNotificationSubscriber`; `wants` only
  `AttentionRequired` (`:388-399`); `deliver` resolves project (`.session.yaml` `project_id`) and
  orchestrator (changeset `orchestrator_session_id`) via `project_of` (`:298-331`), appends under
  the per-project lock (`:401-451`). No project → skipped.
- Registered in `packages/tddy-daemon/src/runtime/mount_host.rs:79-110` (operator channel → stream
  subscriber → project log → embedder's `RuntimeOptions::notification_subscribers`).
- Publish sites: `ActivityServiceImpl::publish_for_session` (`service.rs:164-180`) from
  `report_session_status_impl` (`service/hook_reports.rs:230-233`), `report_agent_activity_impl`
  (`:348`, always `Activity`), `report_operator_notification_impl`
  (`service/operator_notifications.rs`), the presenter observer (`presenter_observer_task.rs:163`
  → `session_notification_publishing.rs:92-103`), and `NotifyOperator` through
  `PeerRoutedActivity` (`exec_tool/project_home.rs:326-327`).
- Status → kind (`session_notifications.rs:236-281`): `WaitingForInput`, `Done` →
  AttentionRequired; `Started`, `Running`, `ExecutingTool` → Activity; `Ended` → nothing. Pending
  question → AttentionRequired/`PendingQuestion` (`:286-315`). Presenter `ModeChanged` awaiting the
  operator → AttentionRequired (`:346-397`).
- Waiters: `tokio::sync::Notify` per project, `notify_waiters()` after append (`:152-158, 452`);
  the wait enables `Notified` before reading (`:263-285`).
- Retention: `PROJECT_NOTIFICATION_CAPACITY = 1_000` (`:53`); file
  `<data dir>/projects/<project_id>/notifications.jsonl`, replayed on open, compacted past 2×
  capacity (`:15-18, 437-450`). Cursor = decimal `seq`; `lost` counts evictions (`:378-386`).
- 20 s cap: `MAX_WAIT_MS = 20_000` (`tddy-tool-engine/src/project_home_inbox_tools.rs:41`),
  `clamp_wait` (`project_notification_log.rs:57-63`).
- Arguments: `cursor`, `timeout_ms`/`limit`, `started_by_me`, `daemon_instance_id`
  (`project_home_inbox_tools.rs:52-105`) — no session-id or kind filter.
- Scope (`project_notification_log.rs:93-104, 143-149`):

  ```rust
  fn visible_to(&self, scope: &ProjectNotificationScope) -> bool {
      self.os_user == scope.os_user
          && self.record.session_id != scope.exclude_session_id
          && scope.started_by.as_ref().is_none_or(|orchestrator| {
              self.record.orchestrator_session_id.as_ref() == Some(orchestrator)
          })
  }
  ```

  Built from `HomeCaller` in `packages/tddy-daemon-rpc/src/exec_tool/project_home_inbox.rs:33-40`.
  One host per call (`:1-7`). `started_by` ignores the host; the answer tools also compare
  `orchestrator_daemon_instance_id` (`answers.rs:355-364`).

### 2. SendSessionMessage

- Tool: `packages/tddy-tool-engine/src/project_home_turn_tools.rs` — definitions `:82-124`, port
  `ProjectHomeTurnPort` `:128-159`, executor `:170-232`, `TurnDelivery::{TerminalPaste,AcpPrompt}`
  `:44-60`.
- Dispatch: `exec_tool/ports.rs:34-102` → `run_locally` (`exec_tool/project_home.rs:117-155`) →
  `project_home_caller` → `DaemonProjectHome::execute` (`:332-359`).
- Daemon: `packages/tddy-daemon-rpc/src/exec_tool/project_home/turn.rs:28-45` (verified first-hand):
  `turn_input_of` → `!is_active` refusal → `refuse_while_waiting_on_an_answer` →
  `terminal_submit_key` → `paste_and_submit`.
- Refusals: `"agent {id} declares no turn_input, so a session of it takes no message"`
  (`turn.rs:100-122`); `"...declares turn_input acp_prompt, which this host cannot deliver yet"`
  (`:92-94`); `"session {id} is waiting on {kind} {request_id:?}: answer it with …"` (`:126-149`).
- Slot: `packages/tddy-coding-agent-specs/src/spec/turn_input_slot.rs:18-28`; accessor
  `CodingAgentSpec::turn_input()` (`spec.rs:354`); `specs/claude.yaml:112`
  `turn_input: { kind: terminal_paste, submit_key: Enter }` — the only spec declaring it.
- `paste_and_submit` (`turn.rs:154-175`): `MAIN_TERMINAL_ID` from `DaemonTerminalSessionStore`,
  bracketed paste when enabled, `terminal.send_input(bytes, 0)`, no lease.
- `ResumeSession` (`turn.rs:49-81`) waits up to 60 s for the terminal to draw (`:178-204`).

### 3. Idle vs mid-turn; the bus

- Status from hooks via `tddy-tools session-hook` → `ActivityService.ReportSessionStatus`; mapping
  `packages/tddy-changeset/src/session_activity.rs:17-31, 109-133` (`SessionStart`→`Started`,
  `UserPromptSubmit`→`Running`, `PostToolUse`→`ExecutingTool`, `Notification`/`PermissionRequest`/
  `PreToolUse(AskUserQuestion)`→`WaitingForInput`, `Stop`→`Done`, `SessionEnd`→`Ended`).
- Stored in `.session.yaml` `activity_status` (`hook_reports.rs:206`; `session_metadata.rs:50-53`),
  with `pending_elicitation` and `pending-question.json`.
- Only `hooks`-capable agents report (claude, cursor, muse) (`hook_reports.rs:185-193`).
- Idle reaper: only `"Running"` is busy (`tddy-agent-launch/src/project_home_idle.rs:28, 49`).
- Bus: `SessionNotificationBus` (`tddy-session-activity/src/session_notifications.rs:74-133`),
  sequential `publish` awaiting each `deliver` (`:109-124`, verified first-hand). Trait
  `SessionNotificationSubscriber { name, wants, deliver, client_relay }` (`:53-71`).
- Attach points: a built-in subscriber in `mount_host.rs`; `RuntimeOptions::
  with_notification_subscriber` (`tddy-daemon/src/runtime.rs:170-177`); the lossy broadcast
  `SessionNotificationStreamSubscriber` (`session_notification_subscribers.rs:20-75`); or waiting
  on `ProjectNotificationLog` (durable, attention-only).

### 4. Orchestrator identity

- `.session.yaml` `session_role` (`tddy-changeset/src/session_metadata.rs:126-129`);
  `PROJECT_HOME_ROLE = "project-home"`, `ORCHESTRATOR_ROLE = "orchestrator"`,
  `is_orchestrator_role` (`:168-183`).
- Main session: project row `home_session_id` (`tddy-projects/src/project_storage.rs:52-55`), set
  by `EnsureProjectHomeSession` (`tddy-agent-launch/src/svc_ensure_project_home.rs`).
- Tool role: `session_tool_role` (`tddy-agent-launch/src/conversation_spawn.rs:21-40`) →
  `SessionToolRole::{ProjectHome, ConversationSpawner, Ordinary}`
  (`tddy-tool-engine/src/project_home_catalogue.rs:109-118`).
- Caller: `ExecuteToolRequest.session_id` → `project_home_caller` → `HomeCaller { session_id,
  project_id }` (`exec_tool/project_home/caller.rs:17-50`; `tddy-worktree-service/src/
  session_target.rs:42-45`). Token → GitHub user → OS user only
  (`tddy-session-agents/src/exec_tool_caller.rs:27-54`).
- Started-by: child changeset `orchestrator_session_id` + `orchestrator_daemon_instance_id`
  (`tddy-changeset/src/changeset/model.rs:95-103`); `require_orchestrator`
  (`tddy-session-activity/src/answers.rs:344-374`).

### 5. Pending questions

- hook → `question_asked_by` (`hook_reports.rs:86-102`) → `pending_question_for_hook` with a new
  UUID `request_id` (`pending_question_record.rs:27-40`) → `record_pending_question`
  (`hook_reports.rs:114-151`) → one `notification_for_pending_question`.
- `PendingQuestion` (`tddy-changeset/src/pending_question.rs:29-56`): `kind`
  (`select`/`multi_select`/`text`/`permission`), `header`, `question`, `options`, `allow_other`,
  `request_id`, `tool_name`, `tool_input_json`, `questions`, `at_unix_ms`.

### 6. Session end

- `SessionEnd` → `Ended`, nothing published (`session_notifications.rs:267`).
- Process exit: `spawn_terminal_cleanup` logs `"main exited: session=… outcome=…"`, runs `on_exit`
  (`ExitHook`), removes the terminal (`tddy-cli-sessions/src/cli_session_manager/
  pty_spawn.rs:243-298`). Not on the bus. Liveness is read lazily from the pid
  (`session_reader.rs:87`).

### 7. Conversation rotation

- `tddy-session-activity/src/service/conversation_rotation.rs:1-113`; `claude.yaml:53`. Marker
  `.conversation-rotation-armed`; new id appended to `agent_conversations`
  (`session_metadata.rs:147-154`). Daemon session id unchanged. `update_activity_status` runs
  before rotation handling (`hook_reports.rs:206-209`).

### 8. Persisted state

- Files only: `.session.yaml` (`deny_unknown_fields`), `changeset.yaml`, `pending-question.json`,
  rotation marker, start ledger; per project `notifications.jsonl` and `stacks/`.
- sqlx only in `tddy-session-catalog` and the model registry. No orchestrator watch concept; near
  matches `MainExitWatch`, `TelegramSessionWatcher`, `OrchestratorPresence`.

### 9. Web

- Nothing reads the project log. Drawer dot: `packages/tddy-session-web/src/rpc/
  useSessionNotifications.ts`, `src/lib/sessionNotificationRegistry.ts`; orchestrator badge
  `SessionDrawerItem.tsx:70,106-110`; main link `tddy-workspace-web/src/projects/
  ProjectMainSessionLink.tsx`.

### 10. Docs

- `docs/ft/daemon/session-notifications.md` (`:37-54`, `:139-182`);
  `packages/tddy-session-activity/docs/session-notifications.md` (`:80-157`);
  `docs/ft/daemon/project-concept.md` (`:197-260`, `:623-651` sending a turn, `:714-742` waiting);
  changelogs `2026-10-29-project-home-waits-for-notifications.md`,
  `2026-11-13-session-turn-control.md`, `2026-11-12/14-orchestrator-sessions.md`.

### 11. Test harnesses (`packages/tddy-daemon-rpc/tests/`)

- `support/mod.rs`: `THIS_PROJECT`, `HOST_A`, `InProcessPeers`, `a_workspace_in`,
  `session_started_by`, `Host { daemon, terminals, sessions }` + `make_project_home`,
  `a_host_logging_notifications` (`:606-629`), `a_host_recording_notifications` (`:633-645`).
- `project_home_wait_acceptance.rs`: `World`/`a_world()` (`:38-67`),
  `a_notification_from_the_child` (`:71-83`), `waits_as` (`:86-106`).
- `project_home_turn_control_acceptance.rs` — the one to reuse for a wake:
  `records_an_agent_session` (`:96-103`), `a_running_session_of` → `RecordingAgent` (a raw-mode
  shell recording typed bytes, `:107-205`), `asks_a_question` (`:113-131`),
  `a_pasted_and_submitted(text)` (`:49-57`), `call`/`assert_answered`/`assert_refused_saying`
  (`:214-280`).

### Patterns run

`WaitProjectNotifications|wait_project_notifications|ReadProjectNotifications`;
`pub enum SessionNotificationKind|SessionNotificationSource|struct QuestionNotice`;
`ProjectNotificationLog::open|with_subscriber(|\.publish(`; `turn_input|TurnInput`;
`session_role|orchestrator_session_id|home_session_id`; `on_exit|ExitHook|main exited`;
`is_active|is_pid_alive`; `capabilities:|turn_input|rotation` in `specs/*.yaml`;
`struct .*Watch\b|struct .*Subscription\b`; `sqlx` in `Cargo.toml`s;
`StreamSessionNotifications|ReadProjectNotifications` in web packages.

## Exploration 2 — first-hand verification and Step 2b scan, 2026-11-16

- Read `turn.rs:20-50`, `ports.rs:25-105`, `session_notifications.rs:100-160, 225-285`; grepped
  `turn_input|project_home_main|capabilities` across `specs/*.yaml` — all confirm Exploration 1.
- Code issues in path (none carries `Claimed by:`):
  `packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-execute-tool.md` (69 > 60
  lines; a response decorator must not be added inline),
  `packages/tddy-daemon/docs/code-issues/complexity-mount-host-build-session-host.md` (73 > 60;
  wiring a new subscriber belongs in `mount_host.rs:79-110`, not `build_session_host`),
  `packages/tddy-cli-sessions/docs/code-issues/complexity-pty-spawn-spawn-tool-on.md` (exit path
  neighbour). `tddy-changeset` has no `docs/code-issues/` — not analyzed.
- TODOs read: `2026-11-13-acp-turn-input-is-not-delivered.md` (bounds who can be woken),
  `2026-10-23-project-home-running-heuristic.md` (pid liveness heuristic — a wake must decide
  "alive" from the served terminal), `2026-11-14-conversation-rotation-residual-drift.md`,
  `2026-11-15-split-agent-exit-cause-unknown.md` (exit is logged since #905, not published),
  `2026-10-29-spawner-read-scope-is-same-host-only.md` (same one-host limit a watch inherits),
  `2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md`,
  `2026-08-29-session-notifications-three-follow-ups-left-open.md` (unrelated).

## Exploration 3 — this node: the seams a watch uses, 2026-11-16

- **Done and WaitingForInput are told apart only by prose.** `notification_for_activity_status`
  (`tddy-session-activity/src/session_notifications.rs:236-281`) raises both as
  `AttentionRequired`/`ActivityStatus` with `question: None`; only `text` differs (`✅ … finished this
  turn` vs `🔔 … needs your input`). The developer chose six kinds, so the status is carried
  structurally (`activity_status`).
- **Tool answers reach the agent as `result_json`, untouched, on every transport.**
  `tddy-session-tool-client/src/rpc_transports.rs:229-233` returns `result_json` (or an error object)
  for unary; the streaming path reassembles it (`:345-366`); the relay wraps it (`:150-156`). A key
  the daemon inserts into the result object therefore reaches the agent on all of them. Precedent:
  `exec_tools.proto:68-71` — a conversation's mutating call's result "gains `worktreeChange`".
- **`execute_tool` branches** (`tddy-daemon-rpc/src/exec_tool/ports.rs:34-102`): peer-forwarded
  (`rpc_served_by_peer`), hosted clone, caller held elsewhere
  (`serve_project_home_for_caller_held_elsewhere`), local. The caller-held-elsewhere branch serves a
  session whose own host forwarded the call; attaching there would hand one host's watches to a
  call another host answers to the agent — so that host attaches, not this one.
  `StreamExecuteTool` is a separate RPC (`exec_tools.proto:31`), used by the split path.
- **Tool families.** `tddy-tool-engine/src/project_home_inbox_tools.rs` is the model: name consts, an
  `is_*_tool` predicate, `*_tool_defs()` via `tool_def`, a port trait implemented on
  `DaemonProjectHome` (`tddy-daemon-rpc/src/exec_tool/project_home_inbox.rs`, whose `scope_of` builds
  the visibility scope from `HomeCaller`).
- **Role.** `session_tool_role` (`tddy-agent-launch/src/conversation_spawn.rs:21-40`) gives
  `SessionToolRole::ProjectHome` to every `is_orchestrator_role` session — the main and
  `session_role: orchestrator` alike. `exec_tool/role_guard.rs` refuses project-home tools to other
  callers.
- **Turn input wording.** `turn_input_of` (`project_home/turn.rs:100-122`): `agent {id} declares no
  turn_input, so a session of it takes no message`; `acp_prompt` is refused in
  `terminal_submit_key` (`:92-94`).
- **Log internals the store needs.** Records are read through `ProjectNotificationScope`, which
  excludes the caller's own session — so "the orchestrator's agent ended" must be read with an
  unscoped reader. The head `seq` gives a new watch its starting position. Eviction past 1 000 is
  what `lost` measures (`project_notification_log.rs:378-386`).
- **Where the store opens.** `tddy-daemon/src/runtime/mount_host.rs::open_shared` opens the log and
  returns it on `Shared` (`:115-122`); the store sits beside it.
