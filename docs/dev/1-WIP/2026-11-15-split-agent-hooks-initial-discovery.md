# Initial Discovery — split agent hooks (`#split-alive` 2/4)

Stack `#split-alive`: a split session (agent on daemon A, codebase on daemon B) that comes up, reports
when it does not, reports its status, can be inspected, and shows its pairing. Source investigation:
branch `investigate/split-session-agent-not-running`,
`docs/dev/todo/2026-11-15-split-session-agent-not-running.md` (copied into n1).

## Combined Conclusions

**State A (verified).**

1. **The split agent dies unseen.** `spawn_split_agent` (`packages/tddy-session-split/src/svc_spawn_split_agent.rs:53-185`)
   spawns the PTY and returns `Ok` immediately (`:178`). Nothing watches the process: no startup
   window (the daemon has one, `spawn_startup_grace_period_ms`, `tddy-daemon-kernel/src/config.rs:45`,
   `:132`, used only by `tddy-spawn/src/spawner.rs:583`), no exit log, and no terminal recording.
   `record_main_capture` is called by the co-located path (`tddy-agent-launch/src/claude_cli_spawn.rs:7-17`)
   and the jailed path (`jail_launch_steps.rs:25`), never by the split spawn
   (`svc_spawn_split_agent/split_agent_pty.rs:17-35`, `launch.rs:97-131`).
2. **The PTY drops the exit status.** `tddy-pty/src/runtime.rs:353-354` maps every `child.wait()`
   `Ok(_)` to `TaskStatus::Completed { exit_code: Some(0) }`: the real code and the signal are lost.
   The cleanup task (`tddy-cli-sessions/src/cli_session_manager/pty_spawn.rs:241-295`) removes the
   terminal without a log line. `stop_session` does log (`terminals.rs:160-178`).
3. **A failed agent step already tears down B.** `split_claude_cli_start.rs:169-200`: `Err` from
   `spawn_split_agent` → `tear_down_codebase_session(.., PeerAnswered)`. A liveness failure returned
   as `Err` from `spawn_split_agent` gets the teardown for free; but the spawned process must be
   stopped by the gate itself (nothing kills it on that path today).
4. **Acceptance harness exists.** `packages/tddy-daemon/tests/remote_managed_worktree_cross_host_acceptance.rs`
   runs two daemons over the LiveKit testkit with a stub `claude` (`a_claude_stub`, `:163`, reads stdin
   forever). `a_failed_agent_spawn_tears_down_the_workspace_session_on_the_codebase_daemon` (`:973`)
   overrides A's binary via `split_hosts_with_agent_claude_binary`. Stub builders:
   `tddy-testing-commons/src/stub_scripts.rs` (`then_running`, `then_reading_stdin`,
   `then_sleeping_secs`).
5. **Split hooks never load.** Hooks are rendered to `<context>/.claude/settings.local.json`
   (`launch.rs:74-95`, `:133-172`, slot `hooks: Some(..)`, adapter `claude-settings-local`). The
   `host_agent` variant passes `--strict-mcp-config --setting-sources user`
   (`tddy-coding-agent-specs/specs/claude.yaml:166-168`). Experiment (2026-11-15, Claude 2.1.311):
   with `--setting-sources user` a `settings.local.json` `UserPromptSubmit` hook does **not** fire;
   with `user,project,local` it does. `AgentVariant` (`spec.rs:372-379`) has only `args` and
   `withdraw_native`; `variant_args` (`:397-424`) renders them.
6. **Split env misses `run.env`.** `split_agent_pty.rs:30` passes
   `base.agent_env(spec, std::env::vars_os(), &[])`; the co-located launch adds the spec's `run.env`
   (`CLAUDE_CODE_NO_FLICKER=1`, `claude.yaml:32`) via `plan_host_pty` (`tddy-agent-launch/src/spec_launch.rs:261`, `:310-314`).
7. **Inspection tools refuse a worktree-less record.** `resolve_target` (`tddy-tool-engine/src/project_home_tools.rs:379-389`)
   → `resolve_session_target` (`tddy-worktree-service/src/session_target.rs:68-89`, `NoWorktree` `:61`).
   See Exploration 2 for every consumer.
8. **Start result reports the agent half's empty branch/worktree.** `tddy-tool-engine/src/start_ledger.rs:325-355`
   looks up `response.session_id` on `record.daemon_instance_id`.
9. **List enrichment drops the orchestrator link for CLI agents.**
   `tddy-session-activity/src/session_list_enrichment.rs:192-203` returns
   `orchestrator_session_id: String::new()` for any session type a coding agent declares. Existing
   TODO: [2026-08-29-a-cursor-cli-session-s-orchestrator-link-never-reaches-sessionentry.md](../todo/2026-08-29-a-cursor-cli-session-s-orchestrator-link-never-reaches-sessionentry.md).
10. **The drawer collapses a dead agent half.** `connectionStatusForSession` → `"disconnected"`;
    `SessionDrawerGroups.tsx:228-236` renders Remaining `defaultOpen={false}`.

**Deferred-work cross-check (Step 2b).**

| Record | Verdict | Node |
|---|---|---|
| `docs/dev/todo/2026-11-15-split-session-agent-not-running.md` (investigation branch only) | during — the stack's own source; carried into n1's plan commit, ✅ RESOLVED by n1 (lowest), referenced by n2–n4 | n1 claims |
| `docs/dev/todo/2026-08-29-a-cursor-cli-session-s-orchestrator-link-never-reaches-sessionentry.md` | during — same early return n4 removes | n4 claims ✅ |
| `docs/dev/todo/2026-11-14-muse-hooks-not-wired.md` | unrelated (Muse agent, not split) | — |
| `docs/dev/todo/2026-08-14-a-split-session-s-room-is-not-re-opened-when-the-daemon-restarts.md` | unrelated to these slices | — |
| `packages/tddy-session-split/docs/code-issues/complexity-svc-spawn-split-agent-spawn-split-agent.md` (233 lines, unclaimed) | n1 grows this function's path; n1 must not grow it — new code goes in its own module | n1 references |
| `packages/tddy-session-split/docs/code-issues/complexity-split-claude-cli-start-start-split-claude-cli-session.md` (CRAP 156, unclaimed) | n1 does not need to touch it (teardown already on `Err`) | — |
| `packages/tddy-pty/docs/code-issues/oversized-file-runtime.md` (523 > 500), `complexity-runtime-spawn-child-and-pump.md` (67 lines, unclaimed) | n1 changes the exit monitor inside `spawn_child_and_pump`; must not grow the file — extract the status mapping | n1 references |
| `packages/tddy-cli-sessions/docs/code-issues/complexity-pty-spawn-spawn-tool-on.md` | n1 logs in `spawn_terminal_cleanup`, not `spawn_tool_on` | — |
| `packages/tddy-coding-agent-specs/docs/code-issues/complexity-slots-mod-render.md` (65 lines) | n2 must not add to `render` | n2 references |

No record is claimed by another running stack.

## Exploration 1 — investigation + parent-driven reads (2026-11-15)

**Sequence.** Mac daemon log `~/.tddy/logs/daemon` (10:12, 10:27–10:31); udoo log
`/var/log/tddy-daemon/daemon`; session dirs on both hosts; `git log` of the split spawn path;
reads listed below; two Claude PTY repro runs and one hooks experiment under `tmp/split-repro/`.

**Inspected files and excerpts.**

- `packages/tddy-session-split/src/svc_spawn_split_agent.rs:53-185` — `spawn_split_agent`; `:144-156`
  `spawn_split_agent_process`, `:158-166` `write_split_agent_metadata`, `:178-184` `Ok(Response)`.
- `packages/tddy-session-split/src/svc_spawn_split_agent/launch.rs:14-72` — hook token, answer wait,
  `write_split_hooks`, `render_split_argv`, `place_pointer_and_spawn`; `:97-131` pointer + PTY spawn,
  pointer removed on spawn error.
- `packages/tddy-session-split/src/svc_spawn_split_agent/split_agent_pty.rs:17-35`:
  ```rust
  cli_manager.spawn_rendered_main(pty.session_id, pty.session_type, pty.context_dir, pty.model,
      pty.argv, base.agent_env(spec, std::env::vars_os(), &[]), pty.os_user, pty.on_exit).await
  ```
- `packages/tddy-session-split/src/split_claude_cli_start.rs:169-200` — agent step and teardown.
- `packages/tddy-session-split/src/agent_argv.rs:126-186` — `split_claude_extra_args`: variant args,
  slots render with `mcp_server`, `allowed_tools`, `denied_tools`, `permission_prompt_tool`.
- `packages/tddy-cli-sessions/src/cli_session_manager/capture_recorder.rs:21-28` (`MAIN_CAPTURE_RECORDING`,
  `main_capture_recording_path`), `:76-100` (`record_main_capture`), `:129-140` (`finish_main_recording`).
- `packages/tddy-cli-sessions/src/cli_session_manager/pty_spawn.rs:75-149` (`spawn_tool_on`),
  `:241-295` (`spawn_terminal_cleanup`), `:384-401` (`finish_spawned_terminal` log).
- `packages/tddy-pty/src/runtime.rs:200-245` (ready signal, cancel → SIGTERM/SIGKILL pid only),
  `:350-367` exit monitor:
  ```rust
  let status = match child_monitor.wait() {
      Ok(_) => TaskStatus::Completed { exit_code: Some(0) },
      Err(e) => TaskStatus::Failed { message: format!("child wait error: {e}") },
  };
  ```
- `packages/tddy-task/src/task.rs:42-58` — `TaskStatus { Completed { exit_code }, Failed { message }, Cancelled }`.
- `packages/tddy-daemon-kernel/src/config.rs:40-50`, `:125-140` — startup grace/poll.
- `packages/tddy-spawn/src/spawner.rs:556-600` — `StartupWatch { grace, poll }`, `from_config`.
- `packages/tddy-coding-agent-specs/specs/claude.yaml` — `run.env` `:32`, `variants.host_agent` `:166-168`.
- `packages/tddy-coding-agent-specs/src/spec.rs:362-424` — `HOST_AGENT_VARIANT`, `AgentVariant`, `variant_args`.
- `packages/tddy-agent-launch/src/spec_launch.rs:261-330` — `plan_host_pty`, `spawn_host_pty` env.
- `packages/tddy-agent-launch/src/project_home_idle.rs:100-150` — auto-stop logs and stops only the
  home session; `tddy-cli-sessions/.../terminals.rs:160-178` `stop_session` logs per task.
- `packages/tddy-daemon/tests/remote_managed_worktree_cross_host_acceptance.rs:150-168`, `:384-490`,
  `:966-995` — harness and the failed-spawn teardown test.
- `packages/tddy-testing-commons/src/stub_scripts.rs` — stub builder.
- `packages/tddy-tool-engine/src/start_ledger.rs:86-110`, `:325-355`.
- `packages/tddy-session-activity/src/session_list_enrichment.rs:185-206`.
- `packages/tddy-session-web/src/SessionDrawerGroups.tsx:40-47`, `:206-236`;
  `utils/connectionStatusForSession.ts:21-22`; `jailedCodebaseHalf.ts:24-30`;
  `useSessionsDrawerSessionList.ts:77-92`.

**Grep / glob.** `record_main_capture` (3 call sites, none split); `spawn_startup_grace_period`
(config + `tddy-spawn` only); `\.stop_session(` callers; `orchestrator_session_id` with
stop/kill (none); `resolve_target(` (12 call sites); `NoWorktree`; `withdraw_native`;
`claude-settings-local`; code-issue listings for the nine packages in scope.

**Experiments.**

- Claude 2.1.311 in a PTY with the split argv shape, allow-listed env and a copy of the real context
  dir: starts, answers, stays up (not the cause by itself).
- `--setting-sources user` vs `user,project,local` with a `settings.local.json` hook: NO vs YES.

**Timing evidence for the exit cause (open).** test-split-udoo-3 agent spawned 10:12:19, home
auto-stopped 10:12:30. test-split-udoo-4 agent spawned 10:28:48, dead by 10:30:49; home stops at
10:23:00 and 10:33:30. No code path from stopping home to the split agent was found. n1 makes the
next failure self-reporting.

## Exploration 2 — split targets and pairing (Explore agent, 2026-11-15)

**A. split-targets (n3).**

1. `packages/tddy-worktree-service/src/session_target.rs`: `TargetRecord` (18-26: `session_id,
   project_id, worktree: Option<PathBuf>, branch, base_ref, is_active`), `HomeCaller` (30-33),
   `SessionTarget` (37-44, `worktree: PathBuf`), `SessionTargetError` (48-53). `resolve_session_target`
   (69-98): is-home → find → project → `NoWorktree` at 86-89. No pairing fields.
2. Other `SessionTarget` constructors: `tddy-daemon-rpc/src/exec_tool/stack_git_tools/git_work.rs:176-189`
   (`target_for`, `worktree.unwrap_or_default()`), `:364-371` (push); fixtures `session_git_ops.rs:247`
   `a_target_at` and tool-engine test doubles.
3. **Need the worktree** (→ codebase half): ReadSessionWorktree (`project_home_tools.rs:404,408`),
   DiffSessionWorktree (`:481-486`), git tools (`project_home_git_tools.rs:167-197` →
   `session_git_ops.rs:145-154` `run::run(&target.worktree, ..)`), `git_work.rs:206-212` `worktree_for`.
   **Use session_id / project_id / is_active only** (→ agent half): `port.rs:142-199` `push_identity`;
   `port.rs:221-239` activity/transcript/tool call (`project_home_session_follow.rs:18-19`
   `unified_session_dir_path(sessions_base, &target.session_id)`); `port.rs:244-254` terminate;
   `project_home/terminal.rs:16-71` read/claim/send keys; `project_home/turn.rs:30,51,100,126`;
   `project_home/answer.rs:17-32,52-67`; `project_home/lifecycle.rs:26-43` set mark, `:45-54` delete,
   `:119-137` restore (project's main repo, not `target.worktree`); `is_active` gates in
   `project_home_lifecycle_tools.rs:312,343,463`; `stack_git_tools.rs:220-236` `delegate_conflicts`
   (agent-side, also hits `NoWorktree` today).
4. Port signatures: `ProjectHomePort` (`project_home_tools.rs` 140-184), terminal (`project_home_terminal_tools.rs`
   140-155), turn (`project_home_turn_tools.rs` 137-160), answer (`project_home_answer_tools.rs` 141-169),
   lifecycle (`project_home_lifecycle_tools.rs` 179-223). `resolve_target` call sites:
   `project_home_tools.rs` 400, 477; `project_home_git_tools.rs` 167; `project_home_answer_tools.rs`
   227, 281; `session_follow.rs` 309; `project_home_lifecycle_tools.rs` 294, 339, 459;
   `project_home_terminate.rs` 46; `project_home_turn_tools.rs` 220, 244;
   `project_home_terminal_tools.rs` 224, 361, 385.
5. `target_records()` (`tddy-daemon-rpc/src/exec_tool/project_home/port.rs:305-321`):
   `worktree: Some(row.entry.repo_path).filter(non-empty)`; rows from `project_home.rs:370-378` →
   `listing.rs:14-54` `session_rows` (`SessionRow{entry, branch, base_ref}`); the entry is
   `tddy_session_activity::session_reader::SessionEntry` (`session_reader.rs:10-41`) and **carries
   `codebase_daemon_instance_id` (27) and `codebase_session_id` (29)**, filled at 100-101. No
   `agent_*` fields (only `SessionMetadata`, `tddy-changeset/src/session_metadata.rs:103,114`).
6. **Forwarding happens before `resolve_target`.** `exec_tool/ports.rs:34-104` (`req.daemon_instance_id`
   route, auth, clone, `serve_project_home_for_caller_held_elsewhere`, `run_locally`);
   `project_home.rs:105-138` `run_locally` → `served_on_target_host` (`project_home_remote.rs:241-262`)
   re-issues to the host named by the tool's `daemon_instance_id` argument
   (`tddy-tool-engine/src/project_home_target_host.rs:26-30`, target-tool list 37-64). On the peer
   (`project_home_remote.rs:60-161`) the caller is confirmed by `ListSessions` fan-out (191-235), one
   hop only (78-87), then `resolve_target` runs against the peer's own records.
7. Tests to match: resolver unit tests `session_target.rs:100-183` (`a_record`, `the_home`,
   `resolving_a_session_without_a_worktree_is_refused`); tool-engine fake-port suites
   `tests/project_home_terminal_tools.rs` (`FakeTerminals`), `project_home_turn_tools.rs`,
   `project_home_lifecycle_tools.rs`, `project_home_answer_question_tool.rs`, `project_home_target_host.rs`,
   `session_follow_paging.rs`; daemon acceptance `tddy-daemon-rpc/tests/project_home_remote_targets_acceptance.rs`
   (`two_hosts()`, `diff_session_worktree_diffs_a_session_on_another_host` 458), `project_home_session_tools_acceptance.rs`,
   `project_home_git_tools_acceptance.rs`, `project_home_lifecycle_tools_acceptance.rs`,
   `project_home_terminal_*_acceptance.rs`, `project_home_turn_control_acceptance.rs`. No project-home
   test uses a split record today.

**B. split-pairing (n4).**

1. Start ledger: `StartedSessionDetails::branch_and_worktree` (`start_ledger.rs:98-107`),
   `ending_of_result` (326-355), `apply_ending` (358-367). Only implementation: `StartedSessionListings`
   (`tddy-agent-launch/src/conversation_start.rs:131-185`, wired 70-73) — `ListSessions` on that host
   (peer or local), `.find(session_id).map(|s| (s.branch, s.repo_path))`; the row also has
   `codebase_*`. Tests: `tddy-tool-engine/tests/start_ledger.rs` (`a_scripted_start`, 120);
   `tddy-daemon-rpc/tests/async_start_acceptance.rs` 207, 314.
2. Enrichment: CLI path `session_list_enrichment.rs:191-215` empties `orchestrator_session_id` while
   reading `stack_id`/`branch` from the changeset; non-CLI paths read it at 238-241 and 278-281. Proto
   row starts empty at `session_coordinate_handlers.rs:403`; `codebase_*` copied at 423-424. Tests:
   1122, 1183, 1240 (workflow sessions); CLI 627, 691, 572.
3. Proto `SessionEntry` (`tddy-service/proto/session.proto`): `repo_path=4`, `daemon_instance_id=8`,
   `orchestrator_session_id=21`, `branch=28`, `codebase_daemon_instance_id=29`, `codebase_session_id=30`,
   `session_role=35`, `stack_id=38`. No agent back-pointer.
4. Drawer row: `SessionDrawerItem.tsx` never renders `branch`; label `utils/sessionDrawerLabel.ts:27-42`
   (`repoPath` basename → `workflowGoal` → short id) — a split agent row shows its short id; codebase
   badge prop computed `SessionDrawerGroups.tsx:44-48`.
5. Web data: `sessionManager.ts:196` lists the selected host only; presence rows
   (`utils/crossHostSessions.ts:94-123`, `lib/sessionParticipantMetadata.ts:73-86`) carry `branch`
   but no `codebase_*`. Nothing joins the codebase row to the agent row.
6. Web tests: `cypress/component/SessionsSplitPlacementAcceptance.cy.tsx` (`aSplitSession()`, tests
   111, 122, 134), `SessionsDrawerCrossHostAcceptance.cy.tsx`; helpers `mountWithRpc`
   (`cypress/support/rpc/inMemory.tsx:9`), `anInMemoryRpcBackend()` (`tddy-connectrpc-testkit`).

**Concurrent work.** `feature/session-display-name` (local branch, commit `174478af8`, no PR yet)
renames `StartedSessionDetails::branch_and_worktree` → `listing` returning `StartedSessionListing`
and adds `display_name` to `ListedSession`/`StartLedgerRecord` — the same seam as n4 (B.1).

## Exploration 3 — node-specific: start and resume flags (2026-11-15)

- `split_session.rs:369-401` `prepare_split_agent_wiring` builds `extra_args` with
  `agent_argv::split_claude_extra_args` and `env: Vec::new()`; called by `svc_resume_split_wiring.rs:91`.
- `tddy-agent-launch/src/opencode_cli_spawn/resume.rs:222,231` extends the resume argv with
  `wiring.extra_args` and its env with `wiring.env`.
- `tddy-agent-launch/src/spec_launch.rs:261-285` `plan_host_pty` returns `spec.run.env` as the PTY env.
- `tests/split_agent_wiring_acceptance.rs:150-206` `AFacilitatingDaemon::wires_a_split_agent_with_an_explorer_attached`
  calls `prepare_split_agent_wiring` directly (no daemon); `flag_values(args, flag)` helper `:265`.
- Experiment (Claude 2.1.311): `--setting-sources user --settings <settings.local.json>` → the file's
  `UserPromptSubmit` hook fires.
