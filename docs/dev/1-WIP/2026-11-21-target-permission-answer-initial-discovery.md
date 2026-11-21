# Initial Discovery: A target agent's permission prompt is answered by its orchestrator

**Changeset**: [2026-11-21-target-permission-answer.md](./2026-11-21-target-permission-answer.md)
**Date**: 2026-11-21
**Passes**: 3 (Exploration 1–3 copied from the stack's whole-work discovery)

## Combined Conclusions

A claude-cli session on a Coder target (`mantasi/remote-dev`, probe session
`01a1fcba-5ee1-7240-9d69-a7512de61640`) reaches the daemon only partially. Each gap, root cause and
reproducing test (committed on `investigate/coder-remote-hooks-partial` at `c72d51b58`):

| # | Gap | Root cause | Remote-only? | Reproduction |
|---|---|---|---|---|
| 1 | Permission prompt never reaches the daemon; `pending_question: null`, no `request_id` | `tddy-agent-backend/src/claude_hooks.rs:181` skips every hook that awaits an answer when `remote_log` is set; no answer channel exists back to a target (`remote_hook_follower.rs` doc: "a hook on a target reports, and is never answered") | yes | `tddy-agent-launch/tests/remote_hooks_acceptance.rs` `a_permission_request_of_an_agent_on_a_target_reaches_the_daemon_with_its_tool_and_input` |
| 2 | Tool calls never `running`; `started_unix_ms: 0` | `PreToolUse` installed only for `AskUserQuestion` (`claude_hooks.rs:90`, `#home-inbox` 3/8); on a target not at all (awaits an answer → skipped). Same locally: no local claude-cli session has a `running` row | no | same file, `a_tool_call_of_an_agent_on_a_target_is_reported_before_it_runs` |
| 3 | Transcript has only tool calls | `tddy-service/src/acp_replay.rs:102` merges `acp-transcript.jsonl` (written by the coder presenter only) with `agent-activity.jsonl`; `UserPromptSubmit.prompt` and `Stop.last_assistant_message` reach `report_session_status_impl` (in `hook_fields`) and are dropped. Existing TODO `docs/dev/todo/2026-10-25-claude-cli-transcript-has-no-conversation-text.md` | no | `tddy-session-activity/tests/claude_cli_transcript_messages.rs` |
| 4 | Read of a worktree file prompts | repo's checked-in `.claude/settings.json:17,25` `ask: Read(//home/**)`, `Edit(//home/**)` (since #224); a Coder worktree lives under `/home/coder` and `ask` beats the implicit cwd allow | no (any Linux host) | live only; config, not code — own PR off master, outside the stack |
| 5 | `kill -0` liveness probe `timed_out` after 504 ms | `tddy-agent-launch/src/remote_agent_launch/startup.rs:92` bounds the probe by what is left of `startup_window` (`spawn_startup_grace_period_ms`, default 500, `tddy-daemon-kernel/src/config.rs:45`); a probe that does not answer counts as alive (`:96`). #963 (`65eeff8ef`, branch `investigate/coder-remote-dev-pid-timeout`) only replaced the pid-file wait bound | yes | `tddy-agent-launch/tests/remote_agent_launch_acceptance.rs` `an_agent_that_dies_on_a_slow_target_fails_the_start_though_its_terminal_reports_late` (control: passes with a round trip < window) |
| 6 | Worktree tab never shows the target worktree; Restore cuts a **local** worktree and repoints the changeset | `tddy-worktree-service/src/service.rs:658` `stream_worktree_stats` lists `git worktree list` of the project's local clone; `restore_session_worktree` (`:500`) calls `setup_worktree_for_session_with_integration_base` on the local clone, which rewrites `changeset.worktree`/`repo_path` (`tddy-session-worktree/src/worktree.rs:58-60`) | yes | `tddy-worktree-service/tests/worktrees_rpc.rs` `restore_of_a_target_sessions_worktree_never_cuts_one_on_the_daemons_host` |

Decisions taken with the user (2026-11-21): answer channel = **answer file** on the target; PreToolUse
on every tool **local and target**; worktree actions **run on the target**; settings.json fix = own
PR off master.

Also observed, already recorded and not in scope: the target's `.tddy/session.json` carries the
Mac's `TDDY_DAEMON_SOCKET`, and `agent-system-prompt.md` tells the remote agent to run `tddy-tools
notify` (L3 in `docs/dev/todo/2026-11-19-coder-remote-agent-local-paths.md`).

### Design consequences (from Explorations 2 and 3)

- **Answer channel (n1, n6).** `request_id` is minted only by the daemon (`hook_reports.rs`), and
  `AwaitQuestionAnswer` requires that id, so a target hook cannot ask for its own answer. The
  hold therefore runs **daemon-side**. The follower delivers the waiting line, which carries a nonce
  the hook minted. The report is made through `HookReportDelivery`, which must return the
  `request_id`; today `ActivityHookDelivery` drops the response. The daemon then awaits the same
  `PendingAnswers` waiter the RPC uses, in a spawned task so the reader never stalls, builds Claude's
  decision JSON, and writes `.tddy/answers/<nonce>` through the session's shell (temp file then
  `mv`, because `write_file` is not atomic). The hook polls for that file within
  `answer_wait_secs` plus a margin, and prints it. With no decision (timed out, no orchestrator) the
  file is written empty or never appears, and Claude asks in its terminal.
  `permission_decision_json` / `question_decision_json` live in `tddy-tools`, which no daemon crate
  reaches, so they move to `tddy-session-activity`; `tddy-tools` already depends on it.
  `HookLine::parse` (`splitn(3)`), the git exclude (`/.tddy/hooks.log` only) and
  `remove_remote_hook_log` all need the answers directory and the nonce.
- **Worktree tab (n2, n7).** No proto change. The target session is found from `worktree_path`, or
  from `session_id` for Restore. A port gives "the shell of a session's target" to
  `tddy-worktree-service`. It is implemented in agent-launch over `shell_for_session_target` and
  wired in `tddy-daemon`.

## Exploration 1 — the investigation (parent-driven Grep / Read / live read-only coder ssh)

### Sequence
1. Read `packages/{tddy-agent-launch,tddy-agent-backend,tddy-session-activity}/docs/changesets/2026-11-16-remote-shell-remote-hooks.md` and `docs/dev/todo/2026-11-19-coder-remote-agent-local-paths.md` (prior work).
2. `grep -rn remote_log packages` → carrier selection points: `opencode_cli_spawn.rs:437`, `opencode_cli_spawn/resume.rs:411`, `session-driver/src/hooks.rs:239`, builders in `claude_hooks.rs`, `cursor_hooks.rs`, `muse_hooks.rs`.
3. Read `tddy-agent-backend/src/claude_hooks.rs` (whole builder) and `remote_hook_command.rs`.
4. Read `tddy-agent-launch/src/remote_hook_follower.rs` (HookLine, HookSink, ActivityHookSink, RemoteHookFollowers, follow_command).
5. Read `tddy-session-activity/src/hook_request.rs` (session_status_request, agent_activity_request) and `service/hook_reports.rs` (report_session_status_impl, report_agent_activity_impl, record_pending_question).
6. Read `tddy-changeset/src/agent_activity.rs` (AgentActivityRecord, running/terminal rows).
7. Local session dirs `~/.tddy/sessions/*`: probe has `agent-activity.jsonl` (2 completed rows, call_id `toolu_…`, `started_unix_ms: 0`), no `acp-transcript.jsonl`; 7 recent local claude-cli sessions: 0 `running` rows, no `acp-transcript.jsonl`.
8. Live, read-only `coder ssh mantasi/remote-dev`: `.claude/settings.json` (repo's, with `ask: Read(//home/**)`), `.claude/settings.local.json` (6 printf hooks: Notification, PostToolUse, SessionEnd, SessionStart, Stop, UserPromptSubmit — no PermissionRequest, no PreToolUse), `.tddy/session.json` (Mac socket leak), `.tddy/hooks.log` (UserPromptSubmit with `prompt`, Notification `permission_prompt` "Claude needs your permission", Stop with `last_assistant_message: "probe done"`, Notification `idle_prompt`).
9. `tddy-service/src/acp_replay.rs:76-112` (read_acp_transcript, read_session_transcript merge) and `tddy-daemon-rpc/src/exec_tool/project_home_session_follow.rs` (session_transcript → TranscriptItem kinds user_message / agent_message / tool_call).
10. `tddy-agent-launch/src/remote_agent_launch/startup.rs` (wait_for_agent_start, exits_during_startup), `remote_agent_launch.rs:404-450` (judge_the_start), `tddy-daemon-kernel/src/config.rs:45` (default 500 ms), `git show 65eeff8ef`.
11. `tddy-workspace-web/src/worktrees/SessionWorktreeTab.tsx` (stats via `useSessionWorktreeStats(client, token, projectId, repoPath)`; Clean/Delete/Restore by `worktreePath`/`sessionId`), `tddy-worktree-service/src/service.rs:417-560,650-760`, `tddy-session-worktree/src/worktree.rs:16-60`.
12. Local answer flow: `tddy-tools/src/session_hook.rs:80-235` (`--await-answer`, `AwaitQuestionAnswer`, `permission_decision_json`, decision is the only stdout), `tddy-session-activity/src/answers.rs` (`answer_permission`), `orchestrator_presence.rs`, `answer_wait.rs`.
13. Shell construction for a target: `tddy-agent-launch/src/remote_agent_launch/target.rs:31` `shell_for_target(&SessionTarget)`, `coder_target.rs:89`.

### Key excerpts
- `claude_hooks.rs:84-93` `CLAUDE_HOOK_EVENTS`: SessionStart, UserPromptSubmit, PostToolUse, Notification, PermissionRequest, (PreToolUse, "AskUserQuestion"), Stop, SessionEnd.
- `claude_hooks.rs:176-183`: `awaits_an_answer = event == PermissionRequest || (PreToolUse && AskUserQuestion)`; `if p.remote_log.is_some() && awaits_an_answer { continue; }`.
- `remote_hook_command.rs`: `printf '%s\t%s\t%s\n' '<event>' '<token>' "$(cat | tr -d '\n')" >> '<log>'`.
- `hook_reports.rs:96-145`: a `WaitingForInput` with a tool records a new question (uuid request_id); a `Notification` WaitingForInput keeps an existing question, else records nothing ("a pending question is never written without a request id").
- `hook_reports.rs:276-330`: PreToolUse mints/pairs call_id (tool_use_id when sent) and writes a `running` row with `started_unix_ms = now`; PostToolUse writes the terminal row with `started_unix_ms: 0`.
- `startup.rs:86-97`: `kill -0 {pid}` `.within(deadline.saturating_duration_since(now))` … `.unwrap_or(true)`.
- `service.rs:500-550` restore: `resolve_main_repo` (local clone) → `setup_worktree_for_session_with_integration_base(&main_repo, &session_dir, &base_ref)`.
- `worktree.rs:58-60`: `cs.worktree = Some(worktree_path)`, `cs.repo_path = Some(worktree_path)`, `write_changeset`.

## Exploration 2 — worktree RPCs for a target session (Explore agent, whose write-up was cut off by a classifier, then parent-driven reads)

### Sequence
1. `tddy-service/proto/worktree.proto` (via the agent): 10 RPCs; the tab uses StreamWorktreeStats, CalculateWorktreeSize, CleanWorktree, RemoveWorktree, RestoreSessionWorktree. Only Restore carries `session_id`; the rest are keyed `(project_id, worktree_path)`. `WorktreeRow` = path, branch_label, disk_bytes, changed_files, lines_added, lines_removed, updated_at_unix_ms, stale, size_status, size_calculated_at_unix_ms.
2. `tddy-worktree-service/src/service.rs` (via the agent): `WorktreeServiceImpl { config, tddy_data_dir, user_resolver, worktree_stats_cache, worktree_size_calculator, session_rooms: Arc<dyn WorktreeRoomCloser>, idle_tracker }`, builders `new`, `with_session_rooms`, `with_idle_tracker`, `with_worktree_size_calculator`. Wired in production at `tddy-daemon/src/runtime/mount_host.rs:122`. clean/remove go through `worktrees::{clean,remove}_worktree_under_repo(&main_repo, path)` → `NotListed` = `NotFound` for any path outside the local clone's `git worktree list`. calculate: `worktree_path_is_listed` else `NotFound`. stream: local `project_worktree_snapshot` only.
3. `grep` Cargo.toml: `tddy-worktree-service` depends on `tddy-remote-shell` (and `tddy-core`, `tddy-projects`, `tddy-daemon-extensions`), **not** on `tddy-agent-launch`; `tddy-agent-launch` **depends on** `tddy-worktree-service` (`Cargo.toml:52`) → anything in agent-launch reaches the worktree service only as an injected port.
4. `tddy-agent-launch/src/svc_signal_delete_session.rs:283-314` `remove_target_worktree(shell, worktree)`: `git -C <root> worktree remove --force <wt>` then `worktree prune`, root from `target_checkout_root_of(worktree)` (`.worktrees` parent). `pub(crate)`.
5. `tddy-agent-launch/src/remote_agent_launch/target.rs:31` `shell_for_target(&SessionTarget)` — ssh only; Coder refused there. `opencode_cli_spawn/session_start.rs:94-118` `shell_for_session_target(target, subject, &dyn CoderCredentialSource) -> Result<Arc<dyn RemoteShell>, Status>` — ssh or Coder through the subject's vault (locked vault = FAILED_PRECONDITION).
6. `tddy-session-worktree/src/lib.rs:31` re-exports `setup_remote_worktree` (the remote cut the start uses; `tddy-git/src/remote_checkout.rs`).
7. Web (via the agent): `SessionWorktreeTab` gets `repoPath` from the session entry and matches its row in the project stream by path; `missing` when no row matches → Restore button.

### Findings
- No proto change is needed for the target worktree: the target session is found from the request's `worktree_path` (a session of the caller with `target` set and `repo_path == worktree_path`, project `project_id`) or, for Restore, its `session_id`.
- The service needs a **port** for "the shell of a session's target" (implemented in agent-launch over `shell_for_session_target`, wired in `tddy-daemon`), because the dependency runs agent-launch → worktree-service.
- `service.rs` (808 lines) and `worktrees.rs` (1003 production lines) carry open oversized-file code issues: new code goes in new modules.

## Exploration 3 — the local answer flow and what a target answer channel needs (Explore agent dump, verbatim)

### 1. Sequence (chronological)

1. `ls packages`, and `ls` of the tddy-session-activity, tddy-tools src and tests directories, to locate modules. `wc -l tddy-tools/src/session_hook.rs` gave 505 lines.
2. `grep -rln "await_answer|AwaitQuestionAnswer"` across `*.rs` and `*.proto`, to find every consumer. The first attempt failed on zsh globbing; I re-ran with quoted `--include='*.rs'`.
3. Read `tddy-tools/src/session_hook.rs` (whole file). It holds `SessionHookArgs`, `permission_decision_json`, `try_run_session_hook` and the unit tests.
4. Read `tddy-tools/src/session_hook/await_answer.rs`, `.../question_decision.rs` and `.../transport.rs`. These cover the AwaitQuestionAnswer client, `question_decision_json`, `encode_question_decision` and the transports.
5. `grep` for `AwaitQuestionAnswer|request_id` in `*.proto`, then read `tddy-service/proto/activity.proto` at lines 80-110 and 290-420 for the wire types.
6. Read `tddy-session-activity/src/answer_wait.rs` in full.
7. Read `tddy-session-activity/src/pending_answers.rs`, `orchestrator_presence.rs`, `pending_answers/question.rs` and `service/question_answers.rs`. These are the pending store, the presence port, the question store and the `AwaitQuestionAnswer` handler.
8. Read `tddy-session-activity/src/answers.rs`, and `answers/question.rs` from line 140 on, for `answer_permission` and `answer_question`.
9. Read `tddy-session-activity/src/service/hook_reports.rs` (where `ReportSessionStatusResponse.request_id` is produced) and `service.rs`.
10. Read `tddy-session-activity/Cargo.toml`, `tddy-agent-launch/Cargo.toml`, `tddy-remote-shell/Cargo.toml` and the head of `tddy-session-activity/src/hook_request.rs`.
11. Read `tddy-agent-launch/src/remote_hook_follower.rs` in full.
12. `grep -rn "RemoteHookFollower|HookReportDelivery|ActivityHookSink"` across all `*.rs`, to find every use.
13. Read `tddy-session-lifecycle/src/connection_service/svc_activity_ports/hook_delivery.rs`, `harness_launch.rs` (lines 60-330) and `resume.rs` (lines 200-460). Read `launch_ports.rs` lines 130-170.
14. Read the `ActivityHookDelivery` wiring in `tddy-session-lifecycle/.../handler_state.rs` (225-265) and `svc_host_builders.rs`. Read `session_start.rs` 60-120, `cli_branch_starts.rs` (200-240 and 330-350) and `svc_signal_delete_session.rs` (120-150 and 260-300).
15. Read `tddy-remote-shell/src/lib.rs` (trait, `ExecRequest`, `ExecOutput`, `FileMode`) and `streamed.rs`. Grepped `in_memory.rs` for its public API.
16. Read `tddy-agent-backend/src/remote_hook_command.rs` and `claude_hooks.rs` (lines 1-330). Grepped `muse_hooks.rs` and `cursor_hooks.rs` for `remote_log`.
17. Read `tddy-tools/tests/permission_decision_json.rs` (head), grepped `question_decision.rs` and `session_hook_pending_question.rs`, and read `tddy-tools-cli/tests/session_hook_await_answer_acceptance.rs` in full.
18. Read `tddy-session-activity/tests/permission_answer_acceptance.rs` in full. Read `question_answer_acceptance.rs` lines 156-300 and 375-430. Grepped the fn and struct outlines of answer_wait_unit, pending_answers_unit, cross_host_presence_unit, question_answer_unit, answer_refusal_messages and clear_answered_question_unit.
19. `grep` for callers of `answer_permission|answer_question`. Read `tddy-daemon-rpc/src/exec_tool/project_home/answer.rs` and `svc_activity_ports.rs` (60-130 and 180-300).
20. Read `tddy-session-driver/src/hooks.rs` (120-290), the other `HookReportDelivery` impl.
21. Read `tddy-agent-launch/tests/remote_hooks_acceptance.rs` (1-470) and `acme_target_support/mod.rs`. Read `tddy-testing-commons/src/remote_target.rs` (1-140).
22. Ran `git status` and `git log`, `git show --stat c72d51b58`, and `head` on the untracked discovery doc.
23. Read `tddy-agent-launch/src/cli_agent_steps.rs` (118-135) and `claude_cli_spawn_steps.rs` (125-150). Grepped `PendingQuestion` in `tddy-changeset/src/pending_question.rs` and `hook_event_asks_a_question` in `tddy-changeset/src/session_activity.rs`.
24. Ran `cargo tree --offline -p tddy-session-activity -e normal` and `-i tddy-remote-shell`, and read the `Cargo.toml` greps for tool-engine, session-lifecycle, daemon-rpc, tools and session-driver, to settle the dependency edges.

---
### 2. Grep / glob (pattern, scope, notable hits)

- `await_answer|AwaitQuestionAnswer`, `*.rs` and `*.proto`:
  - tddy-tools: `session_hook.rs`, `session_hook/await_answer.rs`, `tests/permission_decision_json.rs`.
  - tddy-tools-cli tests: `session_hook_await_answer_acceptance.rs`, `session_hook_question_answer_acceptance.rs`.
  - tddy-session-activity: `service/rpc.rs:67`, `service/question_answers.rs`, `answer_wait.rs`, `pending_answers.rs`, `pending_answers/question.rs`, and the tests `permission_answer_acceptance.rs`, `question_answer_acceptance.rs`, `answer_wait_unit.rs`, `cross_host_presence_unit.rs`.
  - tddy-session-lifecycle: `svc_activity_ports.rs:267` (`PeerRoutedActivity::await_question_answer`).
  - tddy-daemon-rpc tests: `cross_host_answer_wait_acceptance.rs`, `project_home_answer_{question,permission}_acceptance.rs`.
  - Also tddy-sandbox-runner `tests/hook_report_relay.rs`, tddy-tool-engine `project_home_answer_tools.rs`, tddy-service `session_activity.rs:39`, and `claude_cli_spawn_steps.rs:131`.
- `RemoteHookFollower|HookReportDelivery|ActivityHookSink`, `*.rs`:
  - Production `RemoteHookFollower::start` is called in exactly one place, `harness_launch.rs:201`.
  - `ActivityHookSink::new` is built at `harness_launch.rs:195` and `tddy-session-driver/src/hooks.rs:248`.
  - `HookReportDelivery` impls are `ActivityHookDelivery` (`svc_activity_ports/hook_delivery.rs:15`) and `RecordingDelivery` (`tddy-session-driver/src/hooks.rs:149`).
  - The field `hook_report_delivery` is in `launch_ports.rs:153`, wired at `handler_state.rs:247`.
  - `remote_hook_followers` is in `launch_ports.rs:150`, `connection_service.rs:159` and `svc_host_builders.rs:98`.
  - `svc_signal_delete_session.rs:137` calls `stop_and_remove_log`.
- `answer_permission|answer_question`, `*.rs`: the only production caller is `tddy-daemon-rpc/src/exec_tool/project_home/answer.rs:24` and `:62` (`DaemonProjectHome`, the `ProjectHomeAnswerPort` impl). The port is declared in `tddy-tool-engine/src/project_home_answer_tools.rs:142` and `:161`.
- `remote_log.is_some()`: `tddy-agent-backend/src/claude_hooks.rs:181`. `remote_log` in cursor_hooks is at `cursor_hooks.rs:37`.
- `REMOTE_HOOK_LOG`: `tddy-agent-launch/src/opencode_cli_spawn.rs:437`, `resume.rs:411` and `tddy-session-driver/src/hooks.rs:239`, each setting `remote_log = Some(".tddy/hooks.log")` (relative). Also `remote_hook_follower.rs:289` and `:312`, `tddy-testing-commons/src/remote_target.rs:132`, and `tddy-agent-backend/src/lib.rs:57` (re-export).
- `tddy-tools|tddy-session-activity|tddy-agent-launch|tddy-remote-shell` in Cargo.toml files: see section 7.
- `cargo tree --offline -p tddy-session-activity -e normal -i tddy-agent-launch` returned "did not match any packages", so session-activity does not depend on agent-launch even transitively. `-i tddy-remote-shell` showed it is reached transitively through `tddy-git` (and tool-engine).

---
### 3. Inspected files and key excerpts

### 3.1 Local hook: `tddy-tools/src/session_hook.rs`

CLI args, `session_hook.rs:50-91`: `--session`, `--daemon`, `--daemon-socket`, `--os-user`, `--hook-token`, `--event`, `--await-answer`, and `--await-timeout-secs` (default `DEFAULT_AWAIT_TIMEOUT_SECS = 125`, line 95).

Main flow, `try_run_session_hook` at `session_hook.rs:166-214`:
```rust
let reported = report_session_status(&args, &stdin_buf).await?;
if args.await_answer {
    let request_id = reported.map(|response| response.request_id).unwrap_or_default();
    let tool_name = parsed.as_ref().and_then(|ev| ev.tool_name.as_deref());
    if !request_id.is_empty() {
        let decision = match (hook_event_name, tool_name) {
            ("PermissionRequest", _) => await_decision(&args, &stdin_buf, request_id).await?,
            ("PreToolUse", Some(ASK_USER_QUESTION_TOOL)) => await_question_decision(&args, &stdin_buf, request_id).await?,
            _ => None,
        };
        if let Some(decision) = decision { println!("{decision}"); }   // the only stdout
    }
}
```
- The hook is fail-quiet. `run_session_hook` (line 159) swallows every error and the process exits 0.
- `report_session_status` (`:242-260`) POSTs `ReportSessionStatus` through `transport::exchange` with a 2 s `REPORT_TIMEOUT`, and decodes `ReportSessionStatusResponse`. The local hook uses `.request_id` (field 2) only to key `AwaitQuestionAnswer`.
- `report_agent_activity` is called first (`:189`), independent of the status report.

`permission_decision_json`, `session_hook.rs:109-140` (`pub`, `#[must_use]`):
```rust
pub fn permission_decision_json(answer: &AwaitQuestionAnswerResponse, permission_suggestions: Option<&serde_json::Value>) -> Option<String> {
    if AwaitQuestionOutcome::try_from(answer.outcome) != Ok(AwaitQuestionOutcome::Answered) || answer.cancelled { return None; }
    let decision = match answer.option_id.as_str() {
        "allow_once"   => json!({ "behavior": "allow" }),
        "allow_always" => match permission_suggestions {
            Some(s) => json!({ "behavior": "allow", "updatedPermissions": s }),
            None    => json!({ "behavior": "allow" }) },
        option if option.starts_with("reject_") => json!({ "behavior": "deny", "message": answer.message }),
        _ => return None,
    };
    Some(json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision": decision}}).to_string())
}
```
- `await_decision` (`:218-236`) reads `permission_suggestions` from the hook's stdin JSON and passes the compact JSON string as `permission_suggestions_json`. The daemon uses that only to decide whether to offer `allow_always`.

### 3.2 `tddy-tools/src/session_hook/await_answer.rs` (whole file)

- `await_answer` builds `AwaitQuestionAnswerRequest { session_id, hook_token, os_user, request_id, permission_suggestions_json }` and calls `exchange(args, "AwaitQuestionAnswer", bytes, Duration::from_secs(args.await_timeout_secs))`.
- `await_question_decision` calls `await_answer` with an empty suggestions string. It takes `tool_input` from stdin `.tool_input` and returns `question_decision_json(&answer, &tool_input)`.
- `question_decision_json` (`pub`, line 55): returns `None` unless outcome is `Answered`, otherwise `encode_question_decision(tool_input, &answer.answers)`.

### 3.3 `tddy-tools/src/session_hook/question_decision.rs`

`encode_question_decision(tool_input, answers) -> Option<String>`. It returns `None` if `answers` is empty, `tool_input.questions` is missing, or an answer's `question_index` is out of range. Otherwise it builds:
```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow",
  "updatedInput":{"questions":[...as asked...],"answers":{"<question text>":"<labels joined \", \", then other>"}}}}
```
- `updatedInput` is the whole original `tool_input` plus an `answers` map keyed by question text. The doc says this is verified against Claude Code 2.1.312, and that Claude then skips the dialog.
- Answer string: `selected` labels joined with `", "`, with `other` (if non-empty) appended last.
- `QuestionAnswer` is `tddy_service::proto::activity::QuestionAnswer { question_index: u32, selected: Vec<String>, other: Option<String> }`.
- A question left out of the answers is simply absent from `answers`, so Claude asks it.

### 3.4 `tddy-tools/src/session_hook/transport.rs`

- `exchange(args, method, body, timeout)` picks a transport via `hook_transport`:
  - `TDDY_SANDBOX_TOOL_IPC` env var gives `SandboxIpc`;
  - else `--daemon-socket` gives `DaemonSocket`;
  - else `Http` POSTing `/rpc/{ACTIVITY_SERVICE}/{method}`.
- `ACTIVITY_SERVICE = tddy_service::session_activity::ACTIVITY_SERVICE`.

### 3.5 Proto: `tddy-service/proto/activity.proto`

`ReportSessionStatusResponse` (`~:313-319`):
```proto
message ReportSessionStatusResponse { bool ok = 1;
  // The id of the pending question this report recorded; empty for a report that recorded none.
  string request_id = 2; }
```
`AwaitQuestionAnswerRequest` (`:353-362`): `session_id=1, hook_token=2, os_user=3, request_id=4, permission_suggestions_json=5`.

`AwaitQuestionOutcome`: `UNSPECIFIED=0, ANSWERED=1, NO_ORCHESTRATOR=2, TIMED_OUT=3`.

`AwaitQuestionAnswerResponse`: `outcome=1, option_id=2, cancelled=3, message=4, answered_by=5, repeated QuestionAnswer answers=6`.

The RPC doc (`:92-97`) says the hold lasts until the orchestrator answers or the daemon deadline passes, and that the daemon answers `NO_ORCHESTRATOR` at once when there is no orchestrator active.

### 3.6 Daemon side of `AwaitQuestionAnswer`: `tddy-session-activity/src/service/question_answers.rs`

Entry points: `AnswerWaitPorts { pending: Arc<PendingAnswers>, presence: Arc<dyn OrchestratorPresence> }` (default is a fresh `PendingAnswers` plus `NoOrchestratorIsActive`). The handler is `ActivityServiceImpl::await_question_answer_impl` (`:60`), reached via `service/rpc.rs:67`.

Steps in order:
- Auth: `hook_session_dir(os_user, session_id)`, `hook_session`, then `session_offers(session_type, Capability::AskUserQuestion)` (else `failed_precondition`), then `hook_token` equality (else `permission_denied("invalid hook_token")`).
- Step 1, no orchestrator, `:86-91`:
  ```rust
  if !self.orchestrator_is_present(&session_dir).await {
      return Ok(Response::new(outcome_only(AwaitQuestionOutcome::NoOrchestrator)));
  }
  ```
- `orchestrator_is_present` (`:120-148`):
  - It reads the session's changeset (`read_changeset`). On an error, or an absent or empty `orchestrator_session_id`, it returns false (no orchestrator).
  - Otherwise it computes `orchestrator_host` from `orchestrator_daemon_instance_id`, filtered to not equal `self.ports.daemon_instance_id` (`None` means this host).
  - Then it calls `self.answers.presence.is_active(orchestrator, orchestrator_host)`.
- Step 2: `read_pending_question(&session_dir)` must hold a question whose `request_id == req.request_id` (non-empty). Otherwise `failed_precondition("no pending question '<id>' on session <sid>")`. The request_id check is against the file on disk, so a daemon-minted id is required.
- Step 3: `hold = session_answer_wait(&session_dir, session_type, &self.agents)`. Then:
  ```rust
  if question.kind == "permission" {
      pending.open(&req.session_id, permission_request_view(&question, has_suggestions));
      match pending.wait(&req.request_id, hold).await {
          WaitOutcome::Answered(d) => answered_response(d),
          WaitOutcome::TimedOut   => outcome_only(AwaitQuestionOutcome::TimedOut) }
  } else {
      pending.open_question(&req.session_id, question);
      match pending.wait_for_question(&req.request_id, hold).await { ... answered_question_response ... }
  }
  ```
- `answered_response` (`:180`): `Selected{option_id}` gives `(option_id, false)`, `Cancelled` gives `("", true)`. It sets `message = delivered.message.unwrap_or_default()` and `answered_by = by.name()`.
- `answered_question_response` (`:197`) carries `answers` and `answered_by`.
- `has_permission_suggestions` (`:208`) is true only for a non-empty JSON array. It adds `allow_always` to the options.
- `IsSessionTerminalServed` (`:156`) is peer-only (LiveKit transport) and calls `presence.is_active(&req.session_id, None)`.

What the daemon decides with no orchestrator:
- A missing or empty `orchestrator_session_id` in the changeset gives `NO_ORCHESTRATOR` at once. So does an orchestrator whose main terminal this daemon does not serve, or a peer that is not routable, errors, or times out in 3 s (`PEER_PRESENCE_TIMEOUT`).
- It returns `NO_ORCHESTRATOR` without holding. The hook prints nothing and Claude shows its own prompt.
- The daemon records no waiter for that call. The pending question was already recorded by `ReportSessionStatus`, and it stays recorded.
- A timeout is `TIMED_OUT`, and the pending question stays too.

### 3.7 Presence: `tddy-session-activity/src/orchestrator_presence.rs`

- Trait `OrchestratorPresence { async fn is_active(&self, orchestrator_session_id: &str, orchestrator_daemon_instance_id: Option<&str>) -> bool }`.
- `NoOrchestratorIsActive` always returns false. `PEER_PRESENCE_TIMEOUT = 3s`.
- Production impl is `ServedTerminalPresence` in `tddy-session-lifecycle/src/connection_service/svc_activity_ports/orchestrator_presence.rs`:
  - With `None` it checks `terminals.get_terminal(orchestrator_session_id, MAIN_TERMINAL_ID).await.is_some()`.
  - With `Some(host)` it asks the peer through `forward_to_peer_within(... PRESENCE_RPC ... PEER_PRESENCE_TIMEOUT)`.

### 3.8 Hold duration: `tddy-session-activity/src/answer_wait.rs`

- `ANSWER_WAIT_FILE = "answer-wait-ms"`, written beside `.session.yaml` by `record_answer_wait(session_dir, Duration)`.
- `session_answer_wait(session_dir, session_type, agents)` is the recorded wait, else `agents.for_session_type(..).answer_wait_secs()`, else `DEFAULT_ANSWER_WAIT_SECS`.
- Launch records it at `tddy-agent-launch/src/cli_agent_steps.rs:129` (`record_hook_answer_wait`, via `claude_cli_spawn_steps.rs:132-148`).
- Hook-side bounds (`tddy-agent-backend/src/claude_hooks.rs:98-103`):
  - `AWAIT_CLIENT_MARGIN_SECS = 5` (the hook's own bound is `answer_wait_secs + 5`).
  - `CLAUDE_HOOK_TIMEOUT_MARGIN_SECS = 10` (Claude's `timeout` is `answer_wait_secs + 10`).
- Local hook command, `claude_hooks.rs:~126-150`: `"{cmd} --await-answer --await-timeout-secs {answer_wait_secs+5}"`, with JSON `"timeout": answer_wait_secs+10`.

### 3.9 Pending-answers store: `tddy-session-activity/src/pending_answers.rs` and `pending_answers/question.rs`

All state is in memory only, per daemon, one waiter per `request_id`. A restart drops every waiter. There are two parallel slot sets behind separate `Mutex`es in `PendingAnswers { slots, questions }`. Both are keyed by `request_id: String`.

Permission slots (`Slots`):
```rust
open: HashMap<String, Opened{seq, open: OpenRequest{session_id, request: PermissionRequestView}}>,
waiters: HashMap<String, oneshot::Sender<DeliveredAnswer>>,
delivered: HashMap<String, DeliveredAnswer>,     // answer arrived before wait() began
answered: HashMap<String, AnsweredBy>,           // first answerer; capped ANSWERED_REQUESTS_KEPT = 1024
answered_order: VecDeque<String>, next_seq: u64
```
`DeliveredAnswer { answer: PermissionAnswer, by: AnsweredBy, message: Option<String> }`. `WaitOutcome::{Answered(DeliveredAnswer), TimedOut}`.

Methods (line numbers in `pending_answers.rs`):
- `open(session_id, PermissionRequestView)` (`:130`) replaces any earlier waiter for the same id.
- `open_request(id)` (`:151`).
- `wait(id, deadline)` (`:160`): returns the `delivered` entry if one is present. If nothing is `open` it returns `TimedOut` at once. Otherwise it installs a oneshot and waits with `tokio::time::timeout`. On deadline it closes the request (`open.remove`), so a later answer is refused.
- `answer_with_message(&PermissionAnswer, AnsweredBy, Option<String>) -> Result<(), String>` (`:196`):
  - It refuses with `"permission request '<id>' was already answered by <by>"` or `"no hook is waiting on permission request '<id>'"`.
  - Otherwise it removes `open`, calls `remember_answer`, and sends on the waiter, or inserts into `delivered` if the hook is not yet waiting.
- The `PermissionAnswering` trait is implemented (`answer_as`, `answered_by`, `deferred_to_user` is always false, `pending_request`).

Question slots (`QuestionSlots`, same shape): `DeliveredQuestionAnswers { answers: Vec<QuestionAnswer>, by }`, `QuestionWaitOutcome`, `OpenQuestion { session_id, question: PendingQuestion }`. Methods: `open_question` (`:104`), `open_question_request` (`:125`), `wait_for_question` (`:134`), `answer_question_with(request_id, answers, by)` (`:175`), `question_answered_by` (`:211`). Messages: `"question request '<id>' was already answered by <by>"` and `"no hook is waiting on question request '<id>'"`.

`PermissionAnswer` and `AnsweredBy` come from `tddy-discovery/src/subagent/permission.rs`. `AnsweredBy` is `MainAgent | User(String) | Orchestrator(String)`, and `name()` gives the session id for an orchestrator.

### 3.10 `answers.rs` / `answers/question.rs` (the orchestrator side)

`answer_permission(context, caller, target, answer: PermissionAnswer, message: Option<String>) -> Result<AnsweredPermission, AnswerRefusal>`, `answers.rs:236`. The five checks, in order:
1. `require_orchestrator` (`:358`): changeset `orchestrator_session_id == caller.session_id` and the host equals `caller.daemon_instance_id`.
2. `context.pending.answered_by(&request_id)` gives `AlreadyAnswered`.
3. `read_pending_question(session_dir)` must hold that `request_id`, else `UnknownRequest`. `kind != "permission"` gives `NotAPermission`.
4. `context.pending.open_request(&request_id).filter(|o| o.session_id == target)`, then `offered_option` (the option must be in the `PermissionRequestView.options`). Options are `allow_once`, `allow_always` (only with suggestions) and `reject_once`; a cancel is allowed.
5. `pending.answer_with_message(...)` wakes the waiter. Only after that does it run `record_answer` (an `AgentActivityRecord` with call_id `permission:<request_id>`, result `{"answeredBy","optionId"}`), `clear_answered_pending_question`, and compute `at_unix_ms`.

Types: `AnswerContext<'a> { sessions_base: &Path, this_daemon_instance_id: &str, pending: &PendingAnswers }`, `AnsweringCaller { session_id, daemon_instance_id }`, and `AnsweredPermission { session_id, request_id, answered_by, option_id, cancelled, at_unix_ms }`. The `AnswerRefusal` variants are `NotTheOrchestrator`, `AlreadyAnswered`, `UnknownRequest`, `NotAPermission`, `UnknownOption`, `NotAQuestion`, `InvalidAnswers`, `NoRequestId`, `Io`.

The delivered answer does not need to reach the hook before the call returns; a refusal leaves the hook waiting. Constants: `ALLOW_ONCE`, `ALLOW_ALWAYS`, `REJECT_ONCE`.

`answer_question(context, caller, target, request_id, answers: Vec<QuestionAnswer>) -> Result<AnsweredQuestion, AnswerRefusal>` (`answers/question.rs:141`) mirrors this. Its differences:
- `kind == "permission"` gives `NotAQuestion`; an empty `request_id` gives `NoRequestId`.
- `open_question_request(..)` followed by `validate_question_answers` (index in range, no duplicates, labels offered, single-choice has one answer, no empty answers).
- Delivery is `answer_question_with`.
- `record_answer` uses call_id `question:<request_id>`.

Production callers are `tddy-daemon-rpc/src/exec_tool/project_home/answer.rs:24` and `:62`. `DaemonProjectHome` holds `self.pending_answers` (the Arc shared with `DaemonSessionHost::pending_answers()`), and `self.launch: LaunchSessions`, which has the pub field `remote_hook_followers`. Nothing in the answer path currently touches the follower registry or a shell.

### 3.11 Where `request_id` is produced: `tddy-session-activity/src/service/hook_reports.rs`

- `question_asked_by` (`:90`): if `req.status == WaitingForInput` and `req.tool_name` is non-empty, it calls `pending_question_for_hook(&req.tool_name, &req.tool_input_json, uuid::Uuid::new_v4().to_string(), now)`. So `request_id` is a daemon-minted UUID v4 (`pending_question_record.rs` records the question; `kind` is `permission`, `select` or `multi_select`).
- `record_pending_question` (`:118`): if `asked` is `Some`, `write_pending_question` and `set_pending_elicitation(true)`, returning `PendingChange::Recorded(question)`. A `WaitingForInput` with no tool (the `Notification` after a prompt) keeps the existing structured question (`Kept`). Any other status clears it (`Plain`).
- `PendingChange::recorded_request_id` (`:80`): `Recorded(q)` gives `q.request_id`; `Kept` and `Plain` give an empty string.
- `report_session_status_impl` (`:161`) ends with `Ok(Response::new(ReportSessionStatusResponse { ok: true, request_id: change.recorded_request_id() }))`.
- Which requests carry a tool: `tddy-session-activity/src/hook_request.rs` `session_status_request`. For `PermissionRequest` and for `PreToolUse` of `AskUserQuestion` or `request_user_input` (`hook_event_asks_a_question`, `tddy-changeset/src/session_activity.rs:94`), it sets `hook_event_name`, `tool_name` and `tool_input_json`.
- The same module has `agent_activity_request`, which returns `None` for AskUserQuestion and `request_user_input`.
- Consequence: the local hook learns `request_id` from the response of its own report. A remote hook cannot, so it must supply a nonce.

### 3.12 Remote carrier: `tddy-agent-backend/src/remote_hook_command.rs`

```rust
pub const REMOTE_HOOK_LOG: &str = ".tddy/hooks.log";
pub fn remote_hook_command(event: &str, hook_token: &str, log_path: &str) -> String {
    format!("printf '%s\\t%s\\t%s\\n' {} {} \"$(cat | tr -d '\\n')\" >> {}",
        shell_quote(event), shell_quote(hook_token), shell_quote(log_path))
}
```
- The hook reads the payload on stdin, strips newlines, and appends one line. It uses only `printf`, `cat` and `tr`.
- It writes nothing to stdout and does not wait.
- The log path baked in is the relative `.tddy/hooks.log`, so the agent's cwd must be the worktree.

### 3.13 `claude_hooks.rs`

- `HookCommandParams` (`:28-...`): `tddy_tools_path, daemon_url, session_id, os_user, hook_token, daemon_socket, answer_wait_secs, remote_log: Option<&str>, route_env`.
- `CLAUDE_HOOK_EVENTS` (`:84`): `SessionStart`, `UserPromptSubmit`, `PostToolUse`, `Notification`, `PermissionRequest`, `("PreToolUse","AskUserQuestion")`, `Stop`, `SessionEnd`.
- `hook_entry_for_event` (`:120-150`): if `remote_log` is `Some`, it returns `[{"matcher": matcher, "hooks":[{"type":"command","command": remote_hook_command(...)}]}]` (no `timeout` key, no awaiting).
- `build_claude_hooks_settings`, the skip, at `:176-183`:
  ```rust
  let awaits_an_answer = *event == "PermissionRequest" || (*event == "PreToolUse" && *matcher == "AskUserQuestion");
  if p.remote_log.is_some() && awaits_an_answer { continue; }      // line 181
  ```
- The unit test `build_claude_hooks_settings_emits_all_eight_events` uses `remote_log: None`.

### 3.14 Follower: `tddy-agent-launch/src/remote_hook_follower.rs`

Module doc says: "Nothing travels the other way: a hook on a target reports, and is never answered."

Types:
- `HookLine { event: String, payload: serde_json::Value }`.
- `HookLine::parse(line, expected_token)`: `splitn(3, '\t')` gives event, token, payload. It checks the token first, then `is_installed_event` (`CLAUDE_INSTALLED_EVENTS` / `MUSE_INSTALLED_EVENTS` / `CURSOR_HOOK_EVENTS`), then JSON. Errors: `Malformed | WrongToken | UnknownEvent | BrokenPayload`. A 4th tab field would land inside the payload field, so adding a nonce field needs a parse change.
- `trait HookSink { async fn deliver(&self, line: HookLine); }`. The doc says the next line is not delivered until this returns. `read_stream` awaits `sink.deliver(hook)` inline (`deliver_whole_lines`), so a blocking await inside `deliver` would stall the whole log reader.
- `trait HookReportDelivery`:
  ```rust
  async fn report_session_status(&self, request: ReportSessionStatusRequest) -> Result<(), String>;
  async fn report_agent_activity(&self, request: ReportAgentActivityRequest) -> Result<(), String>;
  ```
  Both discard the response, so the minted `request_id` is lost.
- `ActivityHookSink { delivery: Arc<dyn HookReportDelivery>, session_id, hook_token, os_user }` with `new(delivery, session_id, hook_token, os_user)`. It knows neither the shell nor the worktree path. `deliver` maps with `tddy_session_activity::hook_request::{session_status_request, agent_activity_request}`, the same mapping the local hook uses, and logs a refused report with `log::warn!` without retrying.
- `RemoteHookFollowers` (Clone, `Arc<Mutex<HashMap<String, RemoteHookEntry>>>`) with `RemoteHookEntry { follower: Option<RemoteHookFollower>, shell: Arc<dyn RemoteShell>, worktree: PathBuf }`. These fields are private and there is no accessor. The only methods are `insert(session_id, follower, shell, worktree)` and `stop_and_remove_log(session_id)`. So the registry has the session-id to shell+worktree mapping (as private data), and the daemon-side delete uses it.
- `follow_command(log_path, offset)` is `tail -c +N -f '<log>' & t=$!; (while [ -e '<log>' ]; do sleep 1; done; kill "$t") >/dev/null 2>&1 & wait "$t"`.
- `create_remote_hook_log(shell, worktree) -> Result<String, RemoteShellError>` runs an exec to add `/.tddy/hooks.log` to `info/exclude`, then `shell.write_file(&log, b"", FileMode::OwnerOnly)`. `remove_remote_hook_log` runs `rm -f '<log>'`.
- `RemoteHookFollower::start(shell, log_path, hook_token, sink) -> Self` spawns `follow_log`, which loops over `exec_streamed` with reconnect from the byte offset. `stop(self)` aborts the task.
- Existing tests in the file: HookLine parse unit tests and `follow_command` tests.

### 3.15 Sink / follower in production

- `HookReportDelivery` for the session host is `ActivityHookDelivery { pub host: DaemonSessionHost }`, `tddy-session-lifecycle/src/connection_service/svc_activity_ports/hook_delivery.rs:10-44`:
  ```rust
  self.host.activity_service().report_session_status(Request::direct(request)).await.map(|_| ()).map_err(|status| status.message().to_string())
  ```
  The `|_|` is where the `ReportSessionStatusResponse.request_id` is thrown away. The same applies to `report_agent_activity`.
- `DaemonSessionHost::activity_service()` (`svc_activity_ports.rs:~75`) builds `PeerRoutedActivity { connection, local: ActivityServiceImpl::new(ports).with_question_answers(self.answer_wait_ports()).with_agents(...) }`. So `await_question_answer` is already callable in-process. `answer_wait_ports()` uses `self.pending_answers()`, the one shared registry, plus `ServedTerminalPresence`.
- `PeerRoutedActivity::await_question_answer` (`:267`) is not routed; it delegates to `local`.
- Wiring: `handler_state.rs:246-249` sets `remote_hook_followers: self.remote_hook_followers.clone()` and `hook_report_delivery: Some(Arc::new(ActivityHookDelivery { host: self.clone() }))` into `LaunchSessions` (`launch_ports.rs:150-153`). `LaunchSessions` is also built in `cli_branch_starts.rs:226-227` (start) and `:344-345` (spawned child, `target: None`), and `resume.rs:111-112`.
- The one production call to `RemoteHookFollower::start` is `harness_launch.rs:195-209`, in `launch()` when `remote: Some(HarnessRemote)`. In order:
  ```rust
  // before launch (when remote.hook_token and remote.hook_delivery are both Some):
  let hook_log = create_remote_hook_log(shell.as_ref(), worktree_path).await ...;   // :143-150
  // on_exit wraps: followers.stop_and_remove_log(&exited_session)                  // :153-168
  let launched = launch_on_target(...).await ...;
  // after the agent is up:
  let sink = Arc::new(ActivityHookSink::new(Arc::clone(delivery), session_id, token, os_user));
  let follower = RemoteHookFollower::start(Arc::clone(&shell), &log, token, sink);   // :195-205
  remote.hook_followers.insert(session_id, follower, shell, worktree_path.to_path_buf());
  ```
- What is in scope at `harness_launch.rs:~195` (`launch()`): `session_id`, `worktree_path`, `session_dir`, `os_user`, `shell: Arc<dyn RemoteShell>` (`HarnessRemote.shell`), `hook_token`, `hook_delivery`, `hook_followers`, and `log` (absolute: `worktree.join(".tddy/hooks.log")`). So a sink could be given the shell and worktree there. Today `ActivityHookSink::new` receives none of them.
- `HarnessRemote` is built in two places:
  - Start: `opencode_cli_spawn.rs:~341`, with `hook_token: wiring.hooks.as_ref().map(|h| h.hook_token.clone())`.
  - Resume: `resume.rs:366-376`, with `hook_token: meta.hook_token.clone()`.
  - Resume obtains the shell through `resumed_target_shell` (`resume.rs:125-139`, calling `session_start::shell_for_session_target(target, subject, coder_credentials)`, which dispatches Ssh or Coder). Resume re-sets `remote_log` through `hooks_append_to_the_target_log` (`resume.rs:~403-415`).
  - Start side sets `remote_log` in `log_hooks_on_the_target` (`opencode_cli_spawn.rs:429-440`).
- Delete: `svc_signal_delete_session.rs:137` calls `remote_hook_followers.stop_and_remove_log(session_id)`. It reads the target worktree from the session record by `target_worktree_of(session_dir)`, which is `(metadata.target?, metadata.repo_path?)` (`:267`).
- Answer for the sink: `meta.repo_path` on a target session is the worktree path on the target. The shell can be re-derived from `meta.target` plus `shell_for_session_target`, or fetched from the registry entry.
- `tddy-session-driver/src/hooks.rs` has a second `HookReportDelivery` (`RecordingDelivery`) and `SessionHooks::follow` (`:248-266`). It is used by the session-driver tool, not the daemon, and would need updating if the trait changes.

### 3.16 `RemoteShell` (`tddy-remote-shell/src/lib.rs`)

```rust
#[async_trait]
pub trait RemoteShell: Send + Sync {
    fn address(&self) -> ShellAddress;
    async fn exec(&self, request: ExecRequest) -> Result<ExecOutput, RemoteShellError>;
    async fn exec_streamed(&self, command: &str) -> Result<StreamedExec, RemoteShellError>;
    fn interactive(&self, command: &str) -> InteractiveCommand;
    async fn read_file(&self, path: &str) -> Result<Vec<u8>, RemoteShellError> { /* cat '<path>' */ }
    async fn write_file(&self, path: &str, bytes: &[u8], mode: FileMode) -> Result<(), RemoteShellError> {
        // default impl: exec of
        //   OwnerOnly:     "umask 077; mkdir -p '<dir>' && cat > '<path>'"
        //   TargetDefault: "mkdir -p '<dir>' && cat > '<path>'"
        // with ExecRequest.stdin = bytes, then .success_or_exited()
    }
}
pub enum FileMode { TargetDefault, OwnerOnly }
pub struct ExecRequest { pub command: String, pub stdin: Option<Vec<u8>>, pub deadline: Duration }
//   ExecRequest::new(cmd) (deadline DEFAULT_EXEC_DEADLINE = 60s), .with_stdin(..), .within(Duration)
pub struct ExecOutput { pub status: i32, pub stdout: Vec<u8>, pub stderr: Vec<u8> }
//   .succeeded(stdout), .exited(status, stderr), .success_or_exited() -> Result<Self, RemoteShellError::Exited>
```
- `write_file` is NOT atomic (`cat > path`): a polling reader can see a partial or empty file. To make the answer file atomic for a polling hook, write to a temp name and `mv` through `exec`, or poll for a non-empty file or a sentinel.
- `StreamedExec { stdin, stdout, stderr, exit }`; `ShellOutput` and `ShellStdin` are boxed async read and write.
- Implementations: `SshShell`, `CoderShell`, `LocalShell`, `RecordedShell`. With `test-support`, `InMemoryShell` (`new`, `answers`, `answers_when`, `hangs_on`, `is_unreachable`, `has_file`, `streams` returning a `ScriptedStream`, `calls`, `commands_run`, `file(path)`, `file_mode(path)`) and a `contract` suite.
- `tddy-agent-launch` has it as a normal dependency and a dev-dependency with `test-support`.

### 3.17 Existing tests, local answer flow

All are `#[tokio::test]` unless noted.

**`tddy-session-activity/tests/permission_answer_acceptance.rs`**

Harness, in order of definition:
- `OrchestratorsServedPerHost` (fake `OrchestratorPresence`, with a `here: Mutex<HashSet<String>>`).
- `EverySessionIsLabelled` (a `SessionLabels` impl).
- `HostBuilder`, with `a_host()` as the entry point. Methods: `.whose_child_has_no_orchestrator()`, `.whose_childs_orchestrator_runs_on_another_host()`, `.whose_home_is_not_running()`, `.holding_a_hook_for(Duration)`, `.build() -> AnActivityHost`.
- `AnActivityHost { data_dir, service: Arc<ActivityServiceImpl>, pending: Arc<PendingAnswers> }`.
- `AnActivityHost` methods: `the_hook_awaits() -> AWaitingHook` (spawns `service.await_question_answer(Request::direct(AwaitQuestionAnswerRequest{..}))`), `a_hook_waiting().await` (polls `pending.open_request(REQUEST_ID)` until present), `answers(caller, PermissionAnswer, Option<&str>) -> Result<AnsweredPermission, AnswerRefusal>` (calls `answer_permission` with `AnswerContext`/`AnsweringCaller`), `pending_question()`, `recorded_answer()` (reads `read_agent_activity`, call_id `permission:<REQUEST_ID>`), `child_dir()`.
- `AWaitingHook { waiting: JoinHandle<Result<AwaitQuestionAnswerResponse, Status>> }`, with `.outcome()` and `.outcome_within(Duration)`.
- Helpers: `write_claude_cli_session(&Path)` (writes a full `SessionMetadata`, `session_type: "claude-cli"`, `hook_token: HOOK_TOKEN`, `target: None`), `write_orchestrator(dir, &Orchestrator)` (writes a `Changeset` with `orchestrator_session_id` and optionally `orchestrator_daemon_instance_id`), `the_permission_question()` (a `PendingQuestion{kind:"permission", request_id: "req-cargo-publish", tool_name:"Bash", tool_input_json: ...}`), `selecting(option_id)`.
- Trait `AwaitOutcomeAssertions` has `assert_answered(option_id, by, message)` and `assert_outcome(AwaitQuestionOutcome)`.
- The service is built as `ActivityServiceImpl::new(ActivityPorts{ os_users: Err(unauthenticated), tddy_data_dir, activity: Arc::new(AgentActivityHub::default()), conversation_frames, notifications: None, session_labels, deltas: Arc::new(NoSessionDeltaStores), daemon_instance_id, usage_bindings: Arc::new(|_| None) }).with_agents(builtin registry).with_question_answers(AnswerWaitPorts{pending, presence})`.
- Constants: `OS_USER "ada"`, `THIS_HOST "host-a"`, `HOOK_TOKEN`, `REQUEST_ID "req-cargo-publish"`, `A_LONG_WAIT 30s`, `A_SHORT_WAIT 200ms`, `AT_ONCE 1s`, `THE_HOOK_OPENS_WITHIN 2s`.
- Tests:
  - `an_orchestrated_childs_await_returns_allow_when_its_home_answers_allow`
  - `deny_with_a_message_returns_deny_with_that_message`
  - `with_no_answer_before_the_deadline_the_await_times_out`
  - `a_child_without_an_active_orchestrator_returns_no_orchestrator_at_once` (rstest cases `its_home_is_not_running`, `it_has_no_orchestrator`, `its_orchestrator_runs_on_another_host`)
  - `another_home_an_unknown_id_and_an_already_answered_question_are_refused_and_the_waiter_keeps_waiting`
  - `a_newer_pending_question_survives_the_clear_after_an_answer_to_the_older_one`

**`question_answer_acceptance.rs`**

It has a similar harness: `a_host_whose_child_asks(PendingQuestion) -> AnActivityHost`, `OrchestratorsServedHere`, `AWaitingHook::outcome_at_once`, `the_home_answers(Vec<QuestionAnswer>)`, `the_two_questions()`, `a_permission_question()`, `choosing(index, labels)`, `rust_with_fmt_and_clippy()`, and trait `RefusalAssertions`. Tests:
- `an_orchestrated_childs_question_await_returns_the_home_agents_answers`
- `an_unknown_question_index_an_unoffered_label_or_two_selections_on_a_single_choice_question_are_refused_and_the_waiter_keeps_waiting`
- `answer_session_question_on_a_permission_question_is_refused_naming_answer_session_permission`

**Unit tests in `tddy-session-activity/tests`**
- `answer_wait_unit.rs`: `a_recorded_wait_reads_back_as_recorded`, `a_session_that_recorded_no_wait_reads_back_none`, `the_recorded_wait_is_the_hold_even_when_the_spec_now_says_otherwise`, `a_session_with_no_record_is_held_for_its_agent_specs_answer_wait`, `a_session_of_a_type_no_spec_names_is_held_for_the_specs_default`. The harness is `ARegistry`, `a_registry_with`, `claude_yaml_waiting`, `a_session_dir`.
- `pending_answers_unit.rs`: `an_opened_request_is_open_under_its_id_with_the_session_that_asked`, `an_answer_wakes_the_wait_with_who_answered_and_the_message`, `an_answer_given_before_the_wait_begins_is_the_waits_outcome`, `a_second_answer_is_refused_naming_the_first_answerer`, `an_answer_nobody_waits_for_is_refused_naming_the_request`, `a_wait_past_its_deadline_times_out_and_closes_the_request`, `no_request_is_ever_deferred_to_the_web_user`, `a_wait_replaced_by_a_reopen_leaves_the_new_waiter_open`, `only_the_most_recent_answers_are_remembered`.
- `question_answer_unit.rs`: `answers_that_fit_the_questions_are_accepted` and `..._do_not_fit_are_refused_saying_why`, `an_open_question_is_visible_and_an_answer_wakes_its_waiter`, `an_answer_that_arrives_before_the_wait_is_the_waits_outcome`, `the_first_answer_wins_and_a_second_is_refused_naming_the_first`, `a_wait_that_times_out_removes_the_waiter_and_a_late_answer_is_refused`, `waiting_on_a_question_nobody_opened_times_out_at_once`, `a_question_waiter_is_not_a_permission_waiter`, `the_child_records_the_answer_under_question_colon_the_request_id`, `the_recorded_output_names_who_answered_and_each_answer_leaving_out_absent_free_text`.
- `cross_host_presence_unit.rs`: `the_gate_asks_the_port_about_the_host_only_when_it_is_not_this_one`, `an_orchestrator_on_another_host_is_waited_for_when_the_port_says_it_is_active`, `a_peer_is_told_whether_this_daemon_serves_the_session`. It uses `RecordingPresence`, `AHost` and `a_host_whose_child_is_orchestrated_on`.
- `answer_refusal_messages.rs` and `clear_answered_question_unit.rs`: refusal text and clear-rule tests.

**tddy-tools**
- `tddy-tools/tests/permission_decision_json.rs`: rstest over `permission_decision_json` via `tddy_tools::session_hook::permission_decision_json`. First test: `allow_once_prints_a_plain_allow`; helpers `answered(option_id, message)`, `the_suggestions()`, `decision()`, `printed()`.
- `tddy-tools/tests/question_decision.rs`: tests of `encode_question_decision`, for example `a_single_choice_answer_is_the_label_keyed_by_the_question_text`, `a_multi_select_answer_joins_the_labels_with_comma_space`, `free_text_alone_is_the_answer`, `no_answers_print_nothing`, `an_input_without_questions_prints_nothing`.
- `tddy-tools/tests/session_hook_pending_question.rs`: report-request mapping tests (`a_permission_request_reports_waiting_with_the_tool_and_its_input`, `an_ask_user_question_reports_waiting_with_every_question_as_the_tool_input`, and others).
- Process-level, in `tddy-tools-cli/tests/session_hook_await_answer_acceptance.rs` and `session_hook_question_answer_acceptance.rs`. The harness is a `FakeActivityService` (an `RpcService`) mounted via `tddy_connectrpc::connect_router(RpcBridge::new(MultiRpcService::new(...)))` on a loopback axum server.
  - The fake answers `ReportSessionStatus` with `request_id: "req-cargo-publish"` and `AwaitQuestionAnswer` per `TheDaemonAnswers::{With, Refusing}`.
  - `ADaemon::answering(..)`, `the_await_it_was_asked()`; the hook runs through `assert_cmd` `cargo_bin_cmd!("tddy-tools")` with stdin `A_PERMISSION_REQUEST`.
  - Tests: `an_answered_permission_prints_claudes_allow_decision_json_and_exits_zero` and `a_timeout_or_daemon_error_prints_nothing_and_exits_zero` (rstest cases for TimedOut, NoOrchestrator, daemon refused). The question file has `an_answered_question_prints_the_decision_that_hands_claude_those_answers` and `with_no_answer_before_the_deadline_the_question_hook_prints_nothing`.
- `tddy-daemon-rpc/tests/project_home_answer_permission_acceptance.rs`, `project_home_answer_question_acceptance.rs` and `cross_host_answer_wait_acceptance.rs` exist but I did not open them.

**Remote-hook tests (follower side, reusable harness types)**
- `tddy-agent-launch/tests/remote_hooks_acceptance.rs`:
  - Harness types: `ATarget` (a temp dir plus a stand-in `ssh` shell script written with `tddy_testing_commons::remote_target::write_executable`; `.shell() -> Arc<SshShell>` via `SshShell::through(bin/ssh, "buildbox")`, `.worktree()`, `.hook_log()`, `.holds_a_hook_log()`, `.installed_hook(event)`, `.installed_hook_for_tool(event, tool)`).
  - Others: `a_reachable_target()`, `AStubAgent` plus `an_agent_that_fires_as_it_starts[_with](target, hook_command, payload)` (a stub that pipes a payload into the installed hook command under `sh -c`), `ARunningSession { _launched, follower }`, and `impl ADaemon` with `installs_the_sessions_hooks_on`, `launches`, `follows`, `deletes_the_session`, `hooks_once_delivered(event)`, `activity_once_a_hook_arrives()`.
  - The sink in that harness is `TheSessionsActivity`, a `HookSink` that records `statuses` and `delivered` (`acme_target_support/mod.rs`). `a_daemon()` gives `ADaemon { manager, activity, session_dir, ... }`.
  - Tests: `a_hook_fired_by_an_agent_on_a_target_updates_the_sessions_activity`, `deleting_the_session_removes_the_hook_log_on_the_target`, and two failing reproductions (commit `c72d51b58`): `a_permission_request_of_an_agent_on_a_target_reaches_the_daemon_with_its_tool_and_input` (reproduces gap 1) and `a_tool_call_of_an_agent_on_a_target_is_reported_before_it_runs` (gap 2).
  - Because the real `SshShell` runs through a stand-in, a polling hook and a `write_file` of an answer file would run end to end on this harness without any network.
- `tddy-agent-launch/tests/remote_hook_follower_acceptance.rs` uses an `InMemoryShell` and `TheDaemonsHooks` sink. Helpers: `AFollowedTarget`, `ALoggedHook`, `a_target()`. Tests: `each_logged_hook_is_delivered_once_in_order`, `a_line_with_the_wrong_token_is_dropped`, `an_unknown_event_or_a_broken_payload_is_dropped_and_the_next_line_still_arrives`, `a_stream_that_ends_resumes_from_the_byte_it_reached`, `stopping_the_follower_ends_its_remote_command`.
- `tddy-agent-launch/tests/coder_agent_launch_acceptance.rs:602` and `remote_hook_log_tail_ends.rs` also exercise the follower.
- No test exercises `ActivityHookDelivery` (in tddy-session-lifecycle) directly.

---
### 4. Findings

1. **Local answer flow end to end.**
   - The agent hook runs `tddy-tools session-hook --await-answer`.
   - `ReportSessionStatus` records `.pending-question` (via `write_pending_question`) with a daemon-minted UUID `request_id`. The response carries it back in field 2.
   - The hook calls `AwaitQuestionAnswer(request_id)`. The daemon checks the hook token, then orchestrator presence, then that the on-disk pending question has that id.
   - It then `open()`s or `open_question()`s a waiter in the shared in-memory `PendingAnswers` and `wait()`s for the recorded hold.
   - The orchestrator's tool (`AnswerSessionPermission` or `AnswerSessionQuestion`, through `DaemonProjectHome`) runs `answers::answer_permission` or `answer_question`. They validate orchestrator identity, `already answered`, the pending question, and the offered option or answers, then call `answer_with_message` or `answer_question_with`. That completes the waiter (a oneshot, or a `delivered` stash if the hook is not waiting yet). Then they record the activity row (`permission:<id>` or `question:<id>`) and clear the pending question if it is still that one.
   - The hook receives an `AwaitQuestionAnswerResponse` and prints Claude's decision JSON with `permission_decision_json` or `question_decision_json`.
   - The decision JSON builders live in tddy-tools (a binary-crate lib), which depends on tddy-session-activity. tddy-agent-launch and tddy-session-lifecycle do not depend on tddy-tools. For a shell-only remote hook, the decision text has to be produced on the daemon side, so those functions would have to be moved or duplicated into a crate the daemon crates reach (tddy-session-activity already has serde_json and the proto types).
2. **Daemon decisions with no orchestrator.** `NO_ORCHESTRATOR` at once when the changeset names none, or the named one is not served here (or the peer is absent). No waiter is opened and the pending question stays recorded. `TIMED_OUT` when nobody answers within the recorded hold. After a timeout a late answer is refused (`UnknownRequest`).
3. **`request_id` is produced only on the daemon**, inside `report_session_status_impl`, as a UUID v4 (`hook_reports.rs:90-110`, returned at `:80`/`:~190`). A target hook cannot know it. Two further points follow.
   - The follower path loses it: `HookReportDelivery::report_session_status` returns `Result<(), String>`, and `ActivityHookDelivery` maps the response away with `.map(|_| ())`. A nonce-to-request_id mapping would need that trait method to return the response (or `request_id`), or the daemon would need to look up the pending question after the report.
   - `AwaitQuestionAnswer` insists the `request_id` equals the on-disk pending question's, so a remote bridge would call it with the minted id, not the nonce.
4. **The sink cannot reach the shell today.** `ActivityHookSink` has only the delivery, `session_id`, `hook_token` and `os_user`. `RemoteHookFollowers` holds shell and worktree per session id, but privately and with no getter. At the only production call site (`harness_launch.rs:195-209`) both the shell and the worktree path are in scope, so they could be passed into the sink or the registry could get an accessor. The orchestrator-answering path (`DaemonProjectHome`) has `LaunchSessions.remote_hook_followers`, so it could reach the registry. The nonce would have to be carried either through a delivery-level "hold" or through a stored nonce-to-request map.
5. **Follower constraints.**
   - `HookLine::parse` is `splitn(3, '\t')`, so any nonce must be in the payload or the parser must change. It must also be allowed past `is_installed_event` and the token check.
   - `deliver` is awaited inline per line, so a hold must be spawned (`tokio::spawn`) to avoid stalling the reader.
   - `remote_hook_command` builds one `printf ... >> log` line and has no stdout or wait. An answer-waiting variant must (a) append the line plus nonce, (b) poll `.tddy/answers/<nonce>` (relative to the cwd) with a bound that matches `answer_wait_secs` plus a margin and Claude's `timeout`, and (c) `cat` the file to stdout.
   - `claude_hooks.rs:181` would stop skipping awaiting events for a target, and a `timeout` key would be needed. `build_claude_hooks_settings`' target branch currently never emits one (`hook_entry_for_event` returns early for `remote_log`).
6. **Shell write semantics.** `write_file` is non-atomic `mkdir -p dir && cat > path`, with `OwnerOnly` using `umask 077`. A polling reader could see a partially written or empty file; use a temp-then-`mv` via `exec` or poll for a complete file. `exec` has a 60 s default deadline, which is smaller than `answer_wait_secs` (120 by default); a streamed or polled approach is not constrained by that, but the daemon-side `write_file` is a short call so that is fine.
7. **Cleanup.** `remove_remote_hook_log` removes only the log. `create_remote_hook_log` makes `.tddy/` through `write_file`'s `mkdir -p`. An answers directory would need the same exclude and removal, and the git exclude is `/.tddy/hooks.log` only.
8. **Crate dependency edges (section 7).** `tddy-agent-launch` depends on `tddy-session-activity` and on `tddy-remote-shell` directly. `tddy-session-activity` has no dependency on `tddy-agent-launch` (verified, not in its `cargo tree`), and no direct dependency on `tddy-remote-shell`, though it reaches it transitively through `tddy-git` and `tddy-tool-engine`. So tddy-session-activity cannot reference the `RemoteShell` trait without a new direct dep (no cycle, since remote-shell depends only on tddy-credentials), and cannot reference agent-launch types at all. The natural place for logic using both `PendingAnswers` and the shell is `tddy-agent-launch` (the follower side) or `tddy-session-lifecycle` (`ActivityHookDelivery`). `tddy-session-lifecycle` depends on both `tddy-session-activity` and `tddy-agent-launch` but has no direct `tddy-remote-shell` entry in its Cargo.toml (grep returned nothing); `tddy-daemon-rpc` likewise.

---
### 5. Claude Code decision JSON (verbatim shapes)

PermissionRequest (`session_hook.rs:131-139`, `tests/permission_decision_json.rs`):
```json
{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}
{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow","updatedPermissions":<Claude's permission_suggestions array>}}}
{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"<message>"}}}
```
Mapping: `allow_once` gives allow; `allow_always` gives allow with `updatedPermissions` (if suggestions were present, else plain allow); an option starting `reject_` gives deny with `message`. Cancelled, `TimedOut`, `NoOrchestrator` and an unknown option give no output.

PreToolUse AskUserQuestion (`question_decision.rs`):
```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":{"questions":[...],"answers":{"Which language?":"Rust"}}}}
```
The only stdout the hook writes is the decision. Otherwise it writes nothing and Claude shows its own dialog.

---
### 6. Other relevant details

- `ReportSessionStatusRequest` for a question or permission carries `hook_event_name`, `tool_name`, `tool_input_json`, `agent_conversation_id`, `hook_fields` (`activity.proto:~285-310`). For a target it is built by `ActivityHookSink` through the shared `session_status_request`, so the sink would already produce the same request.
- Session metadata for a target session: `meta.target: Option<SessionTarget>` (Ssh or Coder), `meta.repo_path` is the worktree on the target, and `meta.hook_token`.
- The `answer-wait-ms` file: `record_hook_answer_wait` runs in `cli_agent_steps.rs:129` for every session whose spec has the Hooks or OperatorNotify capability, with no `remote_log` condition, so target sessions already have a recorded wait.
- The existing working-tree documents `docs/dev/1-WIP/2026-11-21-target-parity-whole-work-initial-discovery.md` and `packages/*/docs/changesets/2026-11-16-remote-shell-remote-hooks.md` (named there) describe the prior design.

---
### 7. Cargo.toml edges

- `packages/tddy-session-activity/Cargo.toml` direct deps (tddy-*): tddy-core, tddy-coding-agent-specs, tddy-daemon-kernel, tddy-rpc, tddy-service, tddy-usage, tddy-worktree-service, tddy-projects, tddy-discovery, tddy-session-files, tddy-tool-engine, tddy-daemon-sandbox, tddy-daemon-extensions. Dev: tddy-workflow, tddy-pr-stack, tddy-testing-commons. No `tddy-remote-shell`, no `tddy-agent-launch`. Transitive: `tddy-remote-shell` through `tddy-git` and `tddy-tool-engine`; `tddy-agent-launch` not in its tree (verified with `cargo tree -i`).
- `packages/tddy-agent-launch/Cargo.toml` direct deps include `tddy-session-activity` (`path = "../tddy-session-activity"`) and `tddy-remote-shell` (comment: "The port an agent's slot files and session pointer reach a remote worktree through"). Also tddy-service, tddy-core, tddy-discovery, tddy-pty and others. Dev: `tddy-remote-shell` with `features = ["test-support"]`, `tddy-testing-commons`, `tddy-changeset` with `test-support`.
- `packages/tddy-remote-shell/Cargo.toml`: depends only on async-trait, base64, tokio, tddy-credentials. `test-support` feature gives `InMemoryShell` and `contract`.
- `packages/tddy-tools/Cargo.toml:62`: depends on `tddy-session-activity` (so it already shares `hook_request`).
- `packages/tddy-session-lifecycle/Cargo.toml`: depends on `tddy-session-activity` (`:97`) and `tddy-agent-launch` (`:107`); no `tddy-remote-shell` line.
- `packages/tddy-daemon-rpc/Cargo.toml`: depends on `tddy-session-activity` (`:21`) and `tddy-agent-launch` (`:22`); no `tddy-remote-shell` line.
- `packages/tddy-tool-engine/Cargo.toml:28`: depends on `tddy-remote-shell`.
- `packages/tddy-session-driver/Cargo.toml`: depends on `tddy-agent-launch` (`:27`) and `tddy-remote-shell` (`:33`).
- The dependency direction is therefore `tddy-agent-launch -> tddy-session-activity`; the reverse does not exist.