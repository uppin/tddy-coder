# Baseline: the #remote-shell sweep, before any plan is applied

**Changeset**: [2026-11-16-remote-shell-sweep.md](../2026-11-16-remote-shell-sweep.md)
**Plans**: [plans.md](./plans.md)
**Recorded**: 2026-11-16 on `7f6ed0b54` (`feature/remote-shell/sweep`), macOS (Darwin 25.6.0), working tree clean.

Each package was run on its own with `CARGO_INCREMENTAL=0 ./test -p <pkg>` (every target runs, `--test-threads=1`), one after another, and its `.verify-result.txt` copied out before the next run. `tddy-remote-git-repo` was built first (`./dev cargo build -p tddy-remote-git-repo-cli`), so no failure below is the unbuilt binary. The warm index daemon was running beside the test runs. No workspace-wide build or test was run. CI on #921's head (`01a11360e`) is the Linux reference.

## Rust packages

| Package | Targets run | Passed | Failed | Ignored | Failing targets |
|---|---|---|---|---|---|
| `tddy-accounts` | 15 | 152 | 0 | 0 | none |
| `tddy-worktree-service` | 18 | 175 | 0 | 1 | none |
| `tddy-agent-launch` | 19 | 342 | 0 | 0 | none |
| `tddy-session-lifecycle` | 107 | 966 | **13** | 1 | 3 |
| `tddy-daemon` | 43 | 332 | **2** | 1 | 1 |

### Failing tests, by name

All fifteen are the local-only causes the changeset names. A run after the plans is held against this list by name.

**`tddy-session-lifecycle`**

| Target | Test | Cause |
|---|---|---|
| `claude_cli_from_spec_acceptance` | `an_unsandboxed_claude_cli_start_runs_the_spec_binary_with_the_argv_its_template_renders` | `assert_eq` at `:522`: the argv's `--append-system-prompt-file` path is spelled `/tmp/nix-shell.…` on one side and `/private/tmp/nix-shell.…` on the other (macOS `/tmp` symlink) |
| `sandboxed_codebase_lifecycle_acceptance` | `a_daemon_shutdown_leaves_no_sandbox_runner_behind` | `split agent exited during startup (exit code 1): /bin/cat: illegal option -- -` (BSD `cat`) |
| `sandboxed_codebase_lifecycle_acceptance` | `a_jailed_codebase_session_started_by_an_orchestrator_records_it_in_its_first_changeset` | same `/bin/cat: illegal option -- -` |
| `sandboxed_codebase_lifecycle_acceptance` | `deleting_a_sandboxed_codebase_session_removes_its_paired_checkout_session` | same |
| `sandboxed_codebase_lifecycle_acceptance` | `deleting_a_sandboxed_codebase_session_tears_its_jail_down` | same |
| `sandboxed_codebase_lifecycle_acceptance` | `deleting_a_sandboxed_codebase_session_whose_checkout_is_already_gone_succeeds` | same |
| `sandboxed_codebase_placement_acceptance` | `a_jailed_codebase_session_that_skips_permissions_is_still_jailed` | same |
| `sandboxed_codebase_placement_acceptance` | `a_sandboxed_codebase_session_starts_with_no_common_room_configured` | same |
| `sandboxed_codebase_placement_acceptance` | `a_sandboxed_codebase_start_leaves_its_agent_unjailed` | same |
| `sandboxed_codebase_placement_acceptance` | `a_sandboxed_codebase_start_pairs_its_checkout_with_this_daemon` | same |
| `sandboxed_codebase_placement_acceptance` | `a_sandboxed_codebase_start_places_the_worktree_in_a_local_workspace_session` | same |
| `sandboxed_codebase_placement_acceptance` | `a_specialized_agent_on_a_jailed_codebase_session_is_held_by_the_codebase_half` | same |
| `sandboxed_codebase_placement_acceptance` | `managing_a_jailed_codebase_session_leaves_its_checkout_in_the_jail` | same |

**`tddy-daemon`**

| Target | Test | Cause |
|---|---|---|
| `session_attach_cross_host_acceptance` | `a_cross_host_staged_ref_whose_upload_never_completed_is_refused_and_writes_nothing` | `LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL): failed to start a container: … failed to bind host port 0.0.0.0:54578/tcp: address already in use` |
| `session_attach_cross_host_acceptance` | `a_forwarded_rpc_to_a_peer_that_stopped_answering_fails_within_its_deadline` | the same port bind, on `0.0.0.0:54772/tcp` |

The LiveKit bind picks a port per run, so which tests of a LiveKit-backed suite it hits can differ between runs; the cause is the constant. The other LiveKit-backed targets of both packages passed in this run.

### Targets run (`passed/failed/ignored`)

**`tddy-accounts`**: `unit src/lib` 40/0/0 · `account_link_attempt_lifetime` 4/0/0 · `account_link_poll_pacing` 4/0/0 · `account_linking_acceptance` 17/0/0 · `account_linking_unit` 12/0/0 · `account_resolution_acceptance` 7/0/0 · `accounts_service_acceptance` 18/0/0 · `acting_identity_unit` 8/0/0 · `coder_account_acceptance` 12/0/0 · `local_gh_unit` 8/0/0 · `project_resolved_identity_acceptance` 5/0/0 · `provider_account_defaults_acceptance` 4/0/0 · `provider_accounts_acceptance` 10/0/0 · `provider_routes_acceptance` 3/0/0 · `doc tddy_accounts` 0/0/0

**`tddy-worktree-service`**: `unit src/lib` 29/0/0 · `base_sync_cache_unit` 10/0/0 · `branch_owner_unit` 5/0/0 · `remote_git_livekit_acceptance` 12/0/1 · `remote_git_relay_acceptance` 30/0/0 · `remote_git_service_acceptance` 28/0/0 · `remote_git_ssh_clone_acceptance` 1/0/0 · `remote_git_ssh_serve_acceptance` 5/0/0 · `remote_pack_over_shell_acceptance` 1/0/0 · `stream_read_worktree_file_acceptance` 13/0/0 · `stream_read_worktree_file_rpc_acceptance` 7/0/0 · `stream_worktree_stats_rpc` 4/0/0 · `worktree_files_rpc` 8/0/0 · `worktree_session_actions_acceptance` 3/0/0 · `worktree_size_calculator_acceptance` 7/0/0 · `worktrees_acceptance` 2/0/0 · `worktrees_rpc` 10/0/0 · `doc tddy_worktree_service` 0/0/0

**`tddy-agent-launch`**: `unit src/lib` 276/0/0 · `agent_env_allow_list` 2/0/0 · `clone_askpass_acceptance` 10/0/0 · `clone_ssh_env` 5/0/0 · `coder_agent_launch_acceptance` 4/0/0 · `daemon_tool_host_unit` 3/0/0 · `no_callerless_launch_helpers` 3/0/0 · `opencode_cli_facilitation` 3/0/0 · `remote_agent_launch_acceptance` 6/0/0 · `remote_hook_follower_acceptance` 5/0/0 · `remote_hooks_acceptance` 2/0/0 · `session_git_identity` 4/0/0 · `session_start_deadline` 7/0/0 · `slot_files_over_shell_acceptance` 1/0/0 · `spec_launch_acceptance` 1/0/0 · `spec_launch_conversation_rotation` 2/0/0 · `start_effort_checks` 2/0/0 · `start_keepalive` 6/0/0 · `doc tddy_agent_launch` 0/0/0

**`tddy-session-lifecycle`**: `unit src/lib` 183/0/0 · `acceptance_daemon` 9/0/0 · `acp_agents_from_spec_acceptance` 11/0/0 · `agent_activity_stamping_acceptance` 4/0/0 · `agent_session_status_inference_acceptance` 10/0/0 · `agent_terminal_history_recording_acceptance` 6/0/0 · `agent_terminal_recording_launch_acceptance` 2/0/0 · `agent_terminal_tabs_acceptance` 7/0/0 · `agent_tool_socket_per_user_acceptance` 2/0/0 · `claude_cli_from_spec_acceptance` 6/1/0 · `claude_cli_fullscreen_renderer` 9/0/0 · `claude_cli_jail_from_spec_acceptance` 9/0/0 · `claude_cli_participant_metadata_acceptance` 4/0/0 · `claude_cli_permission_mode_acceptance` 16/0/0 · `claude_cli_session_acceptance` 13/0/0 · `claude_cli_tmux_reattach_acceptance` 11/0/0 · `claude_cli_tmux_session_acceptance` 13/0/0 · `cli_harness_tmux_acceptance` 8/0/0 · `cli_spec_features_acceptance` 9/0/0 · `coder_target_session_acceptance` 3/0/0 · `context_rpc_session_scope_acceptance` 6/0/0 · `context_sync_acceptance` 31/0/0 · `cursor_cli_from_spec_acceptance` 6/0/0 · `cursor_cli_hooks_acceptance` 3/0/0 · `cursor_cli_resume_chat_acceptance` 8/0/0 · `cursor_cli_session_acceptance` 12/0/0 · `delete_session` 2/0/0 · `derived_host_start_acceptance` 1/0/0 · `dormant_session_tabs_acceptance` 5/0/0 · `effective_spawn_branch_acceptance` 9/0/0 · `ensure_project_home_session_acceptance` 29/0/0 · `ensure_project_home_session_replace_edges` 8/0/0 · `harness_clis_from_spec_acceptance` 10/0/0 · `headless_claude_subagent_from_spec_acceptance` 1/0/0 · `headless_subagents_from_spec_acceptance` 4/0/0 · `host_coding_agents` 1/0/0 · `host_without_integrations` 1/0/0 · `launcher_names_no_agent` 2/0/0 · `list_sessions_enriched` 2/0/0 · `list_sessions_managed_codebase_acceptance` 4/0/0 · `managed_codebase_workflow_acceptance` 5/0/0 · `managed_workflow_github_token` 3/0/0 · `managed_workflow_transition` 3/0/0 · `no_agent_name_gates` 6/0/0 · `opencode_cli_session` 23/0/0 · `operator_session_starter_unit` 4/0/0 · `orchestrator_session_acceptance` 5/0/0 · `pr_stack_base_session_spawn_acceptance` 23/0/0 · `project_home_idle_stop_acceptance` 7/0/0 · `refused_delete_keeps_the_session_acceptance` 1/0/0 · `remote_hooks_session_acceptance` 2/0/0 · `remote_managed_worktree_acceptance` 18/0/0 · `remote_target_session_acceptance` 9/0/0 · `resume_session_requires_a_recorded_tool_path` 1/0/0 · `rpc_families_port_acceptance` 2/0/0 · `sandbox_behavior_acceptance` 5/0/0 · `sandbox_terminal_parity_acceptance` 12/0/0 · `sandboxed_claude_cli_acceptance` 5/0/0 · `sandboxed_claude_launch_baseline` 2/0/0 · `sandboxed_cli_harness_acceptance` 3/0/0 · `sandboxed_codebase_lifecycle_acceptance` 0/5/0 · `sandboxed_codebase_placement_acceptance` 14/7/0 · `sandboxed_cursor_cli_acceptance` 4/0/0 · `sandboxed_session_lifecycle_acceptance` 2/0/0 · `session_attach_staging_scope_acceptance` 11/0/0 · `session_branch_conflict_acceptance` 9/0/0 · `session_file_upload_rpc` 9/0/0 · `session_files_service_acceptance` 8/0/0 · `session_notification_bus_unit` 19/0/0 · `session_notification_label_unit` 8/0/0 · `session_notification_presenter_unit` 6/0/0 · `session_notifications_stream_acceptance` 9/0/0 · `session_room_acceptance` 29/0/0 · `session_start_deadline_acceptance` 3/0/0 · `session_sync_livekit_acceptance` 6/0/0 · `session_uploads_rpc` 13/0/0 · `session_workflow_files_rpc` 3/0/0 · `sessions_base_path_mismatch` 1/0/0 · `signal_session` 3/0/0 · `split_session_roster_routing_acceptance` 9/0/0 · `ssh_exec_start_acceptance` 1/0/0 · `stack_node_link_failure_tolerance_acceptance` 1/0/0 · `staging_rpc_acceptance` 7/0/0 · `start_cancel_acceptance` 5/0/0 · `start_phase_acceptance` 3/0/0 · `stream_agent_activity_delta_rpc_acceptance` 6/0/0 · `supervisor_spawn_delegation` 9/0/0 · `task_service_acceptance` 19/0/0 · `terminal_control_acceptance` 10/0/0 · `terminal_input_ack_acceptance` 2/0/0 · `terminal_mode_replay_acceptance` 1/0/0 · `terminal_session_acceptance` 12/0/0 · `terminal_session_usage_acceptance` 1/0/0 · `tmux_new_session_acceptance` 5/0/0 · `tmux_tabs_acceptance` 8/0/0 · `tool_session_git_identity` 6/0/0 · `tool_session_host_socket` 16/0/0 · `tool_session_local_gh` 10/0/0 · `unified_worktree_base_acceptance` 5/0/0 · `usage_context_acceptance` 3/0/0 · `warp_cli_session` 14/0/0 · `workspace_seed_admission_integration` 3/0/0 · `workspace_session_acceptance` 2/0/0 · `workspace_session_deletion_acceptance` 2/0/0 · `workspace_session_start_acceptance` 13/0/0 · `yaml_only_agent_acceptance` 17/0/0 · `doc tddy_session_lifecycle` 0/0/1

**`tddy-daemon`**: `unit src/lib` 77/0/0 · `unit src/main` 0/0/0 · `accounts_stub_daemon_acceptance` 2/0/0 · `code_index_warmup_acceptance` 11/0/0 · `code_navigation_acceptance` 7/0/0 · `code_session_acceptance` 19/0/0 · `coder_deployment_probe` 4/0/0 · `coder_workspace_source` 6/0/0 · `common_room_key_trust_acceptance` 3/0/0 · `daemon_config_service` 28/0/0 · `embedded_runtime` 3/0/0 · `first_login_enrolment_acceptance` 9/0/0 · `host_registry_load` 4/0/0 · `index_daemon_lifecycle_acceptance` 13/0/0 · `livekit_service_registration_acceptance` 2/0/0 · `local_socket_family_wiring_acceptance` 1/0/0 · `local_socket_reachability_acceptance` 2/0/0 · `local_token_uds` 13/0/0 · `multi_host_acceptance` 6/0/0 · `plan_dialog_acceptance` 5/0/0 · `provider_model_probe` 3/0/0 · `provider_route_listing_acceptance` 1/0/0 · `relay_e2e_acceptance` 4/0/0 · `relay_idle_shutdown_acceptance` 1/0/1 · `relay_idle_wired_acceptance` 2/0/0 · `relay_runtime_acceptance` 7/0/0 · `remote_managed_worktree_cross_host_acceptance` 15/0/0 · `removed_agent_config_acceptance` 3/0/0 · `runtime_notification_subscriber_acceptance` 1/0/0 · `runtime_signing_identity_acceptance` 1/0/0 · `server_agent_tool_socket_acceptance` 4/0/0 · `server_options_acceptance` 14/0/0 · `service_registration_acceptance` 6/0/0 · `session_agent_remote_acceptance` 22/0/0 · `session_attach_cross_host_acceptance` 6/2/0 · `session_room_cross_host_acceptance` 4/0/0 · `split_session_resume_acceptance` 12/0/0 · `staging_forwarding_acceptance` 2/0/0 · `startup_stack_migration_acceptance` 1/0/0 · `test_placement` 4/0/0 · `unbundle_endpoint` 3/0/0 · `unbundle_tools_dependency_dropped` 1/0/0 · `doc tddy_daemon` 0/0/0

## `tddy-session-create-web`

| Gate | Result |
|---|---|
| `bun test src` (in the package, through `./dev`) | 104 pass, 0 fail, 123 `expect()` calls, 14 files |
| `bun run typecheck` (`tsc --noEmit`) | exit 2, **252** `error TS` lines: all 252 under `cypress/`, none under `src/` |
| Cypress | not run |

The JS dependencies were already installed in this worktree; nothing was installed for these runs.

## State A, as measured

File rows are production lines to the first `#[cfg(test)]`, and `tddy-tools restructure lines` reads the same number for rows 1, 2, 3, 5 and 6. Function rows are brace-matched, `fn` line to closing brace. "At `3b15ffbc4`" is the same measurement on the stack's merge-base with `origin/master`.

| # | File or function | Changeset "before" | Changeset "now" | Measured now | Measured at `3b15ffbc4` |
|---|---|---|---|---|---|
| 1 | `tddy-agent-launch/src/remote_agent_launch.rs` | new | 579 | **579** (758 with tests) | file absent |
| 2 | `…/connection_service/svc_activity_ports.rs` | 513 | 546 | **546** | 513 |
| 3 | `…/connection_service/svc_host_builders.rs` | 531 | 544 | **544** | 531 |
| 4 | `tddy-session-lifecycle/src/connection_service.rs` | over 500 | +6 | **567** whole file · **442** by `restructure lines` · 15 to the first `#[cfg(test)]` | 558 · 433 (**+9**, not +6) |
| 5 | `tddy-accounts/src/service.rs` | 772 | 790 | **790** | 772 |
| 6 | `tddy-worktree-service/src/remote_git_service.rs` | 1,004 | 1,111 | **1,111** | 1,004 |
| 7 | `tddy-session-create-web/src/useCreateSessionPaneState.tsx` | 495 | 503 | **503** | 495 |
| 8 | `tddy-rust-typescript-tests/gen/host_pb.ts` | 1,402 | 1,734 | **1,762** (the `tddy-client-web` copy too) | 1,402 |
| 9 | `start_cli_harness_session` (`opencode_cli_spawn.rs:55`–`:422`) | 319 | 368 | **368** | **318** |
| 10 | `resume_cli_harness_session` (`opencode_cli_spawn/resume.rs:192`–`:357`) | 157 | 166 | **166** | **136** |
| 11 | `resume_cli_harness` (`resume.rs:37`–`:144`) | 95 | 108 | **108** | **88** |
| 12 | `build_session_host` (`tddy-daemon/src/runtime/mount_host.rs:235`–`:316`) | 73 | 82 | **82** | 73 |
| 13 | `start_cli_harness_from_request` (`cli_branch_starts.rs:147`–`:246`) | 98 | 100 | **100** | **94** |
| 14 | `start_sandboxed_cli_harness_session` (`svc_start_sandboxed_cli_harness_session.rs:95`–`:386`) | 291 | 292 | **292** | 291 |
| 15 | `start_session_on_request` (`tddy-agent-launch/src/svc_start_session_core.rs:64`–`:143`) | 79 | 80 | **80** | 79 |
| 16 | `DaemonSessionHost::new` (`svc_host_builders.rs:33`–`:132`) | 97 | 100 | **100** | 97 |

Where the measurements differ from the changeset:

- **Rows 10, 11, 13** — the changeset's "before the stack" (157, 95, 98) is each record's last row, which is the base of #921 alone. #915, #917 and #920 grew the same functions earlier in the stack without a row of their own. On the stack's merge-base they read **136**, **88** and **94**. Row 9 reads **318** there, one less than the changeset's 319.
- **Row 4** — the stack added nine lines (`remote_hook_followers` and `coder_credentials` with their doc comments), not six. The record's own counter (525 on 2026-10-09) is a script that is not in the repo and was not re-run; the file has no inline `#[cfg(test)] mod { }`, so that counter's rule reads the whole file, 567.
- **Row 8** — 1,762 lines, not 1,734: #921 regenerated it once more after the record's last row.
- **Row 15** is a function of `tddy-agent-launch`, not of `tddy-session-lifecycle`.

Files the plans extract into, measured now (production lines): `opencode_cli_spawn.rs` 422, `opencode_cli_spawn/resume.rs` 387, `cli_branch_starts.rs` 339, `svc_start_sandboxed_cli_harness_session.rs` 448, `svc_start_session_core.rs` **499**, `runtime/mount_host.rs` 482.
