# WIP Changeset: Per-worktree hooks — Claude CLI session activity status

**Date:** 2026-06-13
**Status:** Green phase complete — all 28 tests passing
**Packages:** tddy-core, tddy-service, tddy-daemon, tddy-tools

## What

Extend `claude-cli` session sessions with a granular activity status driven by per-worktree
Claude Code hooks. When the daemon starts a claude-cli session it writes
`.claude/settings.local.json` into the worktree configuring six hooks
(`SessionStart`, `UserPromptSubmit`, `PostToolUse`, `Notification`, `Stop`, `SessionEnd`).
Each hook invokes `tddy-tools session-hook --session <id> --daemon <url> ...` which maps the
event to a granular `SessionActivityStatus` and calls the new gRPC `ReportSessionStatus` RPC.
The daemon writes `activity_status` to `.session.yaml` and surfaces it in `ListSessions`.

## Status set

| Claude hook event | notification_type | activity_status |
|---|---|---|
| `SessionStart` | — | `Started` |
| `UserPromptSubmit` | — | `Running` |
| `PostToolUse` | — | `ExecutingTool` |
| `Notification` | permission_prompt / elicitation_dialog / idle_prompt | `WaitingForInput` |
| `Stop` | — | `Done` |
| `SessionEnd` | — | `Ended` |

## TODO

- [x] Create/update PRD documentation (`docs/ft/daemon/claude-cli-session.md`)
- [x] Create changeset (this file)
- [x] `tddy-core`: `session_activity.rs` — `SessionActivityStatus` enum stub, `activity_status_from_hook` stub, `HookEvent` parse (passing), 15 unit tests (failing: todo!())
- [x] `tddy-core`: `claude_hooks.rs` — `HookCommandParams` struct, `build_claude_hooks_settings` stub, 4 unit tests (failing: todo!())
- [x] `tddy-core`: `session_metadata.rs` — `activity_status`, `hook_token` fields added; `update_activity_status` stub; tests added
- [x] `tddy-service`: `connection.proto` — `ReportSessionStatus` RPC, `ReportSessionStatusRequest/Response`, `SessionEntry.activity_status` field 15
- [x] `tddy-daemon`: `config.rs` — `ClaudeCliConfig.tddy_tools_path`, `ClaudeCliConfig.daemon_url`; config tests (passing)
- [x] `tddy-daemon`: `connection_service.rs` — `report_session_status` stub (todo!()); 6 handler unit tests (failing: todo!())
- [x] `tddy-daemon`: `session_list_enrichment.rs` — `activity_status` field in struct; claude-cli enrichment passing; proto propagation test failing
- [x] `tddy-tools`: `tests/session_hook_cli.rs` — 5 CLI integration tests written (4 failing: subcommand missing)
- [x] **GREEN**: `session_activity.rs` — `as_wire()`, `from_wire()`, `activity_status_from_hook()`
- [x] **GREEN**: `claude_hooks.rs` — `build_claude_hooks_settings()`
- [x] **GREEN**: `session_metadata.rs` — `update_activity_status()`
- [x] **GREEN**: `connection_service.rs` — `report_session_status` handler; hook wiring in `start_claude_cli_session`
- [x] **GREEN**: `session_list_enrichment.rs` — `apply_session_list_status_to_proto` copies `activity_status`
- [x] **GREEN**: `tddy-tools`: `session_hook.rs` + `main.rs` — `session-hook` subcommand with fail-quiet RPC
- [x] Acceptance tests passing
- [x] Unit tests passing

## Red phase summary (28 failing tests)

### tddy-core (17 failing — all `todo!()` panics)
- `claude_hooks::tests::build_claude_hooks_settings_emits_all_six_events`
- `claude_hooks::tests::each_event_has_one_matcher_group_with_one_command_hook`
- `claude_hooks::tests::hook_command_uses_provided_tddy_tools_path`
- `claude_hooks::tests::hook_command_includes_session_token_daemon_event_and_os_user`
- `session_activity::tests::session_start_event_maps_to_started`
- `session_activity::tests::user_prompt_submit_maps_to_running`
- `session_activity::tests::post_tool_use_maps_to_executing_tool`
- `session_activity::tests::notification_permission_prompt_maps_to_waiting_for_input`
- `session_activity::tests::notification_elicitation_dialog_maps_to_waiting_for_input`
- `session_activity::tests::notification_idle_prompt_maps_to_waiting_for_input`
- `session_activity::tests::stop_event_maps_to_done`
- `session_activity::tests::session_end_maps_to_ended`
- `session_activity::tests::unknown_event_is_noop`
- `session_activity::tests::notification_unknown_subtype_is_noop`
- `session_activity::tests::wire_strings_are_stable`
- `session_activity::tests::parsed_notification_event_maps_to_waiting_for_input`
- `session_metadata::tests::update_activity_status_overwrites_only_status_and_bumps_updated_at`

### tddy-daemon (7 failing)
- `connection_service::report_session_status_unit_tests::report_session_status_writes_activity_status_to_session_yaml` — todo!()
- `connection_service::report_session_status_unit_tests::report_session_status_rejects_unknown_session` — todo!()
- `connection_service::report_session_status_unit_tests::report_session_status_rejects_bad_hook_token` — todo!()
- `connection_service::report_session_status_unit_tests::report_session_status_rejects_non_claude_cli_session` — todo!()
- `connection_service::report_session_status_unit_tests::report_session_status_rejects_unknown_status_string` — todo!()
- `connection_service::report_session_status_unit_tests::report_session_status_rejects_session_id_path_traversal` — todo!()
- `session_list_enrichment::tests::apply_session_list_status_to_proto_sets_activity_status` — `apply_session_list_status_to_proto` not setting `activity_status`

### tddy-tools (4 failing — `session-hook` subcommand missing)
- `session_hook_appears_in_help`
- `session_hook_help_lists_required_flags`
- `session_hook_noop_event_exits_zero_without_daemon`
- `session_hook_unreachable_daemon_exits_zero`
