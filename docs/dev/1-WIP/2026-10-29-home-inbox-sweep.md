# Changeset: #home-inbox sweep

**Created:** 2026-10-29
**Status:** 🚧 Green: features, race fix and both splits done; validate and wrap pending
**Type:** Sweep (feature + refactor + fix)
**Complexity**: 5. It covers three independent features, two file splits, and a dependency upgrade (Tauri 2.12 plus the notification plugin) across about eight packages.
**Stack:** `#home-inbox 11/11` (sweep), branch `feature/home-inbox/sweep`, base `feature/home-inbox/cross-host-wait`
**PRD:** [PRD-2026-10-29-home-inbox-sweep.md](../../ft/daemon/1-WIP/PRD-2026-10-29-home-inbox-sweep.md)

## Initial Discovery

[2026-10-29-home-inbox-sweep-initial-discovery.md](./2026-10-29-home-inbox-sweep-initial-discovery.md)

## Prerequisites (what this node resolves)

| Record | Verdict |
|---|---|
| [`docs/dev/todo/2026-10-29-conversation-spawner-cannot-read-its-childs-permission-request.md`](../todo/2026-10-29-conversation-spawner-cannot-read-its-childs-permission-request.md) | ✅ RESOLVED HERE. The developer chose a scoped read tool (2026-10-29). |
| [`docs/dev/todo/2026-10-26-shell-less-sessions-cannot-notify-their-operator.md`](../todo/2026-10-26-shell-less-sessions-cannot-notify-their-operator.md) | ✅ RESOLVED HERE |
| [`docs/dev/todo/2026-10-26-desktop-notification-click-needs-tauri-2-12.md`](../todo/2026-10-26-desktop-notification-click-needs-tauri-2-12.md) | ✅ RESOLVED HERE. The developer approved the Tauri upgrade (2026-10-29). |
| [`packages/tddy-daemon-rpc/docs/code-issues/oversized-file-exec-tool-project-home.md`](../../../packages/tddy-daemon-rpc/docs/code-issues/oversized-file-exec-tool-project-home.md) | ✅ RESOLVED HERE. Split along the record's own seams (B listing, C caller/log, plus the stack's terminal, inbox and answer port impls). |
| [`packages/tddy-tool-engine/docs/code-issues/oversized-file-project-home-tools.md`](../../../packages/tddy-tool-engine/docs/code-issues/oversized-file-project-home-tools.md) | ✅ RESOLVED HERE. Split along seams A (target host), B (catalogue) and C (listing types). |
| `answer_permission`'s unconditional pending-question clear, found at `8/11`'s validate | ✅ RESOLVED HERE |
| [`packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md`](../../../packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md), [`packages/tddy-task/docs/code-issues/oversized-file-terminal-capture.md`](../../../packages/tddy-task/docs/code-issues/oversized-file-terminal-capture.md) | Not in scope. The stack grew them only by single lines (`lib.rs` 885 → 888), or not at all (`terminal_capture.rs` ends at 712, below its base of 728). |

## Affected Packages

- [ ] `tddy-tool-engine`: spawner offer of `ReadSessionActivity`, the `NotifyOperator` tool def and dispatch, and the `project_home_tools.rs` split
- [ ] `tddy-daemon-rpc`: the scoped read check for spawners, `NotifyOperator` execution, and the `exec_tool/project_home.rs` split
- [ ] `tddy-session-activity`: the conditional clear in `answer_permission`
- [ ] `tddy-agent-launch`: guidance for standing-role sessions names `NotifyOperator`
- [ ] `tddy-tools`: the MCP serving and admission pins for `NotifyOperator`
- [ ] `tddy-desktop`: Tauri 2.12, notification plugin 2.5+, and the tap handler
- [ ] `Cargo.lock`

## Boundaries

- Nothing outside the three TODOs, the two splits and the race fix.
- **The splits are pure moves.** No behaviour changes, and they are done with the restructure engine where it accepts the move (`code-restructuring` skill). A move the engine refuses needs the developer's consent; see memory `restructure-refusal-needs-consent`.

## Dependencies

| Parent node | Consumed |
|---|---|
| `2/11`, `5/11`, `6/11`, `7/11`, `8/11`, `9/11` | the files being split; `ReadSessionActivity` gating; `answer_permission` |
| `1/11` agent-notify | `ReportOperatorNotification` and its credential check; the guidance paragraph |
| `4/11` desktop-notify | `native_notifications.rs`, `open_session_route`, `TauriNotifier` |
| `10/11` cross-host-wait | sits below; the splits come after it so they include its edits |

## Draft PR contract

Pass 2 (commit 2) publishes the surface below, plus its failing tests. Every body marked
`TODO(sweep)` fails closed: it grants no read, publishes no notification and opens no route, and
nothing panics. The splits and the wiring land in green.

| Item | Surface (`TODO(sweep)` bodies) | Crate |
|---|---|---|
| Spawner read scope | `project_home_tools::is_conversation_spawner_scoped_tool` (returns `false`); `exec_tool/project_home.rs::refuse_a_spawner_outside_its_own_sessions` (refuses), with a `project_home_caller` arm for scoped tools | tool-engine, daemon-rpc |
| `NotifyOperator` | new `project_home_notify_tools`: `NOTIFY_OPERATOR`, `is_project_home_notify_tool`, `notify_operator_tool_def`, `NotifyOperatorArgs::into_notice` (refuses), `OperatorNotice`, `ProjectHomeNotifyPort` (the default refuses), `execute_project_home_notify_tool` (refuses); `impl ProjectHomeNotifyPort for DaemonProjectHome` (refuses), with a dispatch arm in `DaemonProjectHome::execute` | tool-engine, daemon-rpc |
| Desktop tap | new `native_notifications/tap.rs`: `TAP_ACTION_ID`, `EXTRA_SESSION_ID`, `EXTRA_HOST`, `TappedNotification`, `SessionRoute::tap_extra` (returns an empty map), `session_route_from_tap` (returns `None`), `on_notification_tap` (does nothing) | desktop |
| Race fix | `answers::clear_answered_pending_question` (clears unconditionally, as today) | session-activity |
| Upgrade | tauri 2.11.5 → 2.12.5; `tauri-plugin-notification` `~2.4` → `"2.5"`, which resolves 2.8.0 | `Cargo.lock`, desktop `Cargo.toml` |

**Green also has to:**
- Add `NotifyOperator` to `is_project_home_tool` and to the offered catalogue, the admission pins
  in `tddy-tools` and the main's pre-approved tools. Name it in `project_home_prompt.md`.
- Join `is_conversation_spawner_scoped_tool` to the spawner's offer.
- Register `on_action` in `start_daemon`'s setup, and attach `tap_extra` in `TauriNotifier::show`.
- Call `clear_answered_pending_question` from `answer_permission` and `answer_question`.
- Do both file splits.

## Decisions (6e review, 2026-10-29)

- **NotifyOperator's credential.**
  - It is a daemon exec tool. The exec-tool path authenticates the caller by its session token
    and reads the standing role from the session's own record.
  - The daemon then calls `ReportOperatorNotification` in-process, using the `hook_token` from that
    session's `.session.yaml`. This reuses `1/11`'s path unchanged.
  - The standing-role launch gets no notify environment.
  - A main that is not claude-cli or cursor-cli, or has no hook token, is refused with a message
    that names the tool and the reason. For example: "NotifyOperator: this session has no hook
    token (only claude-cli and cursor-cli sessions can notify)". The raw `session_type` error is
    never surfaced.
- **Race test seam.** The interleave inside `answer_permission` cannot be staged
  deterministically: the hook wakes through a oneshot, and the clear follows synchronously. So the
  race is tested on `clear_answered_pending_question` directly.
- **Spawner scope is this host only, for now.** A spawner's child on another host stays refused.
  See [`docs/dev/todo/2026-10-29-spawner-read-scope-is-same-host-only.md`](../todo/2026-10-29-spawner-read-scope-is-same-host-only.md).
- **The guidance test lives in `tddy-session-lifecycle`**, because that suite observes the main's
  launch prompt.
- **Versions.** tauri 2.11.5 → 2.12.5, the tauri-runtime 2.12 line, tauri-build, codegen and
  macros 2.7.4, tauri-utils 2.10.2, `tauri-plugin-notification` 2.4.0 → 2.8.0, wry 0.55.1 → 0.57.1,
  tao 0.35.3 → 0.37.2, muda 0.20 and tray-icon 0.25. Everything else that moved is inside Tauri's
  tree. The developer approved the upgrade (2026-10-29). `cargo check --all-targets -p tddy-desktop`
  passes on it. **Green must run `tddy-desktop`'s full suite.**
- **`tddy-tool-engine` now declares `tddy-daemon-kernel`.** It is needed for
  `agent_notification_limits`. The crate was already in tool-engine's normal tree through
  `tddy-task`, so the declaration adds no crate.

## Green decisions (AFK, 2026-10-29)

- **Spawner scope check** (`exec_tool/project_home/caller.rs`) re-reads the caller's role from its
  record; only a `ConversationSpawner` is checked. The target is the call's `session_id`; its
  changeset (same host only) must name the caller as `orchestrator_session_id`. A missing, invalid or
  remote target is refused as permission denied, naming the spawner and "started".
- **`NotifyOperator` credential.** `DaemonProjectHome` holds the host's `PeerRoutedActivity`
  (`ProjectHomeHost.activity`, from `DaemonSessionHost::activity_service()`) and calls
  `ReportOperatorNotification` in process with the session's `.session.yaml` `hook_token`. The
  session type (`claude-cli` / `cursor-cli`) and a non-empty token are checked first, so the
  activity service's raw `session_type` error is never surfaced.
- **Level parsing** is case-sensitive over the four names; an omitted or empty level is `info`.
- **Desktop tap.** `register_tap_handler` (in `tauri_host`) is called from `start_daemon` before the
  subscriber is built; the plugin's `extra` map is read as strings, and a tap whose notification the
  platform did not report opens nothing. A failed open is logged, not returned.
- **Race fix** is `clear_answered_pending_question`; `answer_permission` and `answer_question` both
  call it and n8's inline copy is gone.

### Splits (pure moves)

| File | Production lines before | After |
|---|---|---|
| `tddy-daemon-rpc/src/exec_tool/project_home.rs` | 992 | 347 (+ `caller` 119, `listing` 54, `port` 280, `terminal` 102, `notify` 50, `answer` 91) |
| `tddy-tool-engine/src/project_home_tools.rs` | 771 | 484 (+ `project_home_catalogue` 239, `project_home_listing` 30, `project_home_target_host` 61) |

- **All moves were done by the restructure engine** (`move_item`, `restructure verify --against HEAD`
  reports every statement accounted for); **no hand move of code**. Two hand edits after the engine:
  an unused `TerminalControl` import in `terminal.rs`, and `cargo fix`'s removal of 37 unused imports
  in `port.rs` (the engine's tidy did not run on it, because a three-operation plan stopped at its
  third operation with "anchor invalidated: position … fell inside text removed by an earlier
  operation"; that operation, `session_rows`, was re-run as its own plan).
- `exec_tool/project_home.rs` went further than the record's seams: the whole
  `impl ProjectHomePort for DaemonProjectHome` moved to `port.rs` (a trait impl cannot be split), as
  the file stayed at 664 lines without it. Seam B's `rows` (an inherent-impl member) stays in
  `project_home.rs`; `SessionRow` and `session_rows` moved to `listing.rs`.
- `project_home_tools.rs`: seam B (catalogue) also took the offered-catalogue and role functions
  (`is_project_home_tool`, `SessionToolRole`, …), and every moved public item is re-exported by
  name, so no public path changed.
- Both code-issue records are deleted: every file is under 500 production lines.

## Testing Strategy

- **Acceptance:**
  - a spawner reads its child's `request_id` and answers it, and is refused for a session it did not spawn;
  - a project-home main's `NotifyOperator` publishes one agent notification, and is not offered to a session without the role;
  - a tap payload maps to its `SessionRoute`, and the handler calls `open_session_route`;
  - the permission clear keeps a newer pending question.
- **Splits:** the existing suites stay green, unchanged.

### Acceptance tests (pass 2, failing)

| Test | Fails now because |
|---|---|
| `tddy-daemon-rpc/tests/spawner_read_scope_acceptance.rs` `a_grill_me_spawner_is_offered_read_session_activity` | the spawner's offer has no `ReadSessionActivity` |
| … `a_grill_me_spawner_reads_its_childs_pending_request_id_and_answers_it` | `ReadSessionActivity` is "served only to a project-home session" |
| … `a_grill_me_spawner_reading_a_session_it_did_not_start_is_refused_as_permission_denied` | refused, but by the role gate rather than the scope check (it names neither the spawner nor "started") |
| `tddy-daemon-rpc/tests/notify_operator_tool_acceptance.rs` `a_project_home_mains_notify_operator_publishes_one_agent_attention_notification` | `NotifyOperator` is not a project-home tool yet; the role guard refuses it as read-only |
| … `a_project_home_main_is_offered_notify_operator` | not in the project-home catalogue |
| … `a_session_without_a_standing_role_is_not_offered_notify_operator_and_publishes_nothing` | passes (a guard that must stay green) |
| `tddy-session-lifecycle/tests/ensure_project_home_session_acceptance.rs` `an_ensured_main_is_told_to_notify_its_operator_with_the_tool_not_the_cli` | the role prompt does not name `NotifyOperator` |
| `tddy-desktop/src-tauri/tests/native_notifications_acceptance.rs` `a_tap_on_a_notice_maps_back_to_the_route_it_was_shown_for` | `tap_extra` / `session_route_from_tap` are stubs (`None`) |
| … `the_tap_handler_brings_the_window_forward_on_the_tapped_session` | `on_notification_tap` is a stub (no window events) |
| … `an_action_that_is_not_a_tap_and_a_payload_naming_no_session_open_nothing` | passes (a guard) |
| … `a_main_with_no_hook_token_is_refused_naming_the_tool_and_why_and_publishes_nothing` | the role guard refuses it as read-only, which says nothing of a hook token |
| `tddy-session-activity/tests/permission_answer_acceptance.rs` `a_newer_pending_question_survives_the_clear_after_an_answer_to_the_older_one` | `clear_answered_pending_question` still clears unconditionally |

### Unit tests (6f)

| Test file | Failing | Passing (guards) |
|---|---|---|
| `tddy-tool-engine/tests/project_home_notify_tool.rs` | 6: the silent info default, each level plus sound, an unknown level refused naming the four, an empty title, the title limit (200 accepted, 201 refused), the body limit (2,000 / 2,001). `into_notice` is a stub. | `notify_operator_is_the_notify_tool_…`, `a_refusal_names_the_tool` |
| `tddy-tool-engine/tests/conversation_spawner_scope.rs` | 2: `ReadSessionActivity` is scoped, and the spawner's catalogue offers it. The predicate is `false`. | `no_other_project_home_tool_is_scoped_…` |
| `tddy-session-activity/tests/clear_answered_question_unit.rs` | 2: another `request_id` is kept, and a question with no `request_id` is kept. The stub clears unconditionally. | the answered question is cleared; no file is a no-op |
| `tddy-desktop/src-tauri/tests/native_notifications_unit.rs` | 3: the payload keys, the round trip with an awkward host, extra keys ignored. The stubs return an empty map and `None`. | a missing or empty key opens nothing; a non-tap action opens nothing |

## TODO

- [x] Pass 2: the contract surface
- [x] Create the failing acceptance tests
- [x] Run the acceptance tests and check that they fail
- [x] USER REVIEW of the acceptance tests (orchestrator, 2026-10-29)
- [x] TDD Red: write the failing unit and integration tests
- [x] TDD Green, including the splits, the race fix and `tddy-desktop`'s full suite
- [ ] `/validate-changes`, `/pr-wrap`
- [ ] Manual: macOS notification click

## Validation Results

Not yet run.
