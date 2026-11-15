# 2026-11-15 — A split session reports a successful start, but its agent dies unseen and cannot be inspected

**Category:** Known defect
**Source:** investigation `investigate/split-session-agent-not-running`, building on
`investigate/split-session-udoo` (session `01a1db97-…`). Split starts from the project-home session
against `mantas-udoo-ros`: `test-split-udoo-3` (`01a1de43-66d2-7fc1-aefc-2ec505ca48ba`, 10:12) and the
live repro `test-split-udoo-4` (`01a1de51-f08d-79c3-9a76-b5fcbc66089c`, 10:27:54). Both hosts ran
master-era builds: the Mac app was installed 2026-11-15 09:23, and udoo has run the `effad1cf7` build
since 2026-11-14 21:52.

Evidence marks: **[V]** verified (log, disk, code or experiment), **[I]** inferred.

## Summary

The earlier failure (`record_orchestrator` after the spawn, fixed by #878) is gone. Both halves are
now created and the agent process **is** spawned. Then:

1. The agent process dies within seconds and prints nothing anywhere a person can read. No exit is
   logged, and no terminal recording is kept.
2. The start reports `succeeded` anyway. It only checks that `spawn` returned, never that the agent
   lives.
3. Even a living split agent could never report status, questions or notifications. Its hooks are
   written to `.claude/settings.local.json`, but it is launched with `--setting-sources user`, which
   never loads that file.
4. Every project-home inspection tool refuses the agent half ("has no worktree") and does not know the
   id of the codebase half.
5. In the drawer, the dead agent half sits in the project group's **Remaining** section, which is
   collapsed by default, so the session looks missing.

**Still open:** why the Claude process exits. Its output was never captured (item 1), and an
offline reproduction with the same flags and env does **not** exit (see A.3). Capturing it requires one
more split start while the watcher described in [Next step](#next-step-to-close-a) is armed.

---

## A. Was the agent spawned? Where did it stop? Why "succeeded"?

### A.1 It was spawned, then died with no trace [V]

Mac daemon log (`~/.tddy/logs/daemon`), test-split-udoo-4:

```
10:27:57.555 write_changeset_atomic: session_dir=…/01a1de51-…-b5fcbc66089c        (orchestrator, before spawn: #878 works)
10:27:57.575 agent tool wiring: … codebase_session=01a1de51-…-b602f420269d tddy_tools=…/tddy-tools relay=livekit
10:28:11.499 split session 01a1de51-… facilitated in session-01a1de51-…, measuring session …-b602f420269d on daemon mantas-udoo-ros
10:28:48.634 context_sync: "…/context" fetched 80 path(s)                          (37 s: the 54 s start vs 18 s on -3)
10:28:48.757 spawn_tool: registered session=01a1de51-… terminal=main kind=claude-cli pid=96609
10:28:48.772 started split claude-cli session 01a1de51-… pid=96609 …
```

After that the log has nothing for this session or pid: no exit line and no `stop_session`. The
filtered window 10:28:48–10:30:30 has only peer RPC traffic. test-split-udoo-3 (pid 38063, 10:12:19)
shows the identical sequence.

Process and disk state:

- pid 96609 was already gone at about 10:31, when it was first checked. pid 38063 was also gone.
- `context/.tddy/` is empty. The session pointer is removed only when the agent exits
  (`svc_spawn_split_agent/launch.rs:58-60`, `:108`), so the process ended.
- `stop_session` always logs `stop_session: session=… — cancelling task`
  (`tddy-cli-sessions/src/cli_session_manager/terminals.rs:168-176`), and no such line exists. So the
  daemon did not kill it: the process exited on its own [I from absence].
- **Claude never got as far as its first turn:**
  - There is no transcript `~/.claude/projects/-Users-mantasi--tddy-sessions-01a1de4{3,51}…-context/`.
    A healthy run writes one as soon as it starts its first turn (repro in A.3).
  - There is no `~/.claude.json` project entry for either context dir. Older working split contexts
    (`01a0dd54…` through `01a180c8…`, the last on 2026-10-28) all have one.
  - There is no `tddy-tools.mcp.log`, although `claude-mcp-config.json` sets `TDDY_TOOLS_LOG_FILE`.
    The MCP server was never started.
  - There is no crash report in `~/Library/Logs/DiagnosticReports`.
- udoo saw no tool call from the agent. Its log for 10:28:45–10:31 holds only the Mac's
  `GetWorktreeSnapshot` polls (97) and two orchestrator `ListSessions` forwards. udoo is healthy: the
  worktree was cut at 10:27:57 and 80 context paths were served at 10:28:12/16. Its only warning is the
  known index-daemon `/run/tddy-index-daemon.sock: Permission denied`.
- The same holds for the 2026-11-14 orphans `01a1db70-…` and `01a1db9e-…`: no transcript, no project
  entry, no MCP log.

**Every split agent since 2026-10-28 has died this way.** The split spawn path changed in between:
#799 (2026-11-06) added `--setting-sources user` and the permission-prompt tool, #833 (2026-11-09)
moved the remote to the tool server and the session pointer, and #836 (2026-11-10) introduced the
allow-listed agent env. [V for dates and PRs; I for which one is responsible]

### A.2 The output is lost by design: a split agent's terminal is never recorded [V]

A main terminal's output survives its process only when someone calls `record_main_capture`. That
call is made by `tddy-agent-launch/src/claude_cli_spawn.rs:16` (co-located claude-cli) and
`tddy-agent-launch/src/jail_launch_steps.rs:25` (jailed). The split spawn
(`tddy-session-split/src/svc_spawn_split_agent/split_agent_pty.rs:17-35`, called from
`launch.rs:110`) never calls it. When the process exits, `finish_main_recording`
(`capture_recorder.rs:129-140`) finds no registration, and the terminal leaves the index
(`pty_spawn.rs:267-292`) without a log line. Neither session dir has `terminals/main.capture`.
Whatever Claude printed before it exited is gone.

### A.3 The flags and env alone do not kill it [V, experiment]

Claude 2.1.311 was run in a PTY (`script`) under a trusted path (`tmp/split-repro/`). It used the
split agent's argv shape: `--model … --session-id … --permission-mode auto --strict-mcp-config
--setting-sources user --mcp-config … --allowedTools … --disallowedTools … --permission-prompt-tool
mcp__tddy-tools__approval_prompt <prompt>`, the allow-listed base env only (`PATH HOME USER LOGNAME
SHELL TERM LANG TMPDIR`), and a copy of the real context dir. Run with the real `tddy-tools --mcp`
binary and no credentials, it started, answered and stayed up until it was killed after 50 s. It wrote
its transcript and started the MCP server.

What this does **not** reproduce:

- the real `claude-mcp-config.json`, whose `tddy-tools` joins the LiveKit room with a live token;
- the daemon's own PTY runtime (`PtyEnvBase::Cleared`) and its process parent;
- the 1,000-character multi-line prompt;
- the exact `--allowedTools` list.

The cause lies in one of these.

A run outside the trusted tree (`/private/tmp`) instead stops on Claude's trust dialog, whose
default is **"No, exit"**. That does not apply here: `/Users/mantasi` is trusted in `~/.claude.json`,
so `~/.tddy/sessions/*/context` inherits the trust. It is still a hazard for any host whose sessions dir
is not under a trusted folder [I].

Smaller divergence found on the way [V]: the split spawn computes its env as
`base.agent_env(spec, std::env::vars_os(), &[])` (`split_agent_pty.rs:30`). It never adds the spec's
`run.env` (`CLAUDE_CODE_NO_FLICKER=1`, `specs/claude.yaml:32`), which the co-located launch adds via
`plan_host_pty` (`spec_launch.rs:261`, `:310-314`). This changes the renderer, not liveness.

### A.4 Why the start says `succeeded` with empty branch and worktree: a missing error [V]

- `spawn_split_agent` returns `Ok` as soon as `spawn_split_agent_process` returns a handle
  (`svc_spawn_split_agent.rs:144-185`). Nothing checks that the process is still alive, has opened its
  MCP server, or has reached its first turn. A process that exits 100 ms later is a successful start.
  **This is the missing error.**
- `branch`/`worktree` are empty because the start ledger looks up the **agent** half
  (`tddy-tool-engine/src/start_ledger.rs:340-348`, `branch_and_worktree(record.daemon_instance_id,
  response.session_id)`). That half has `repo_path: null` by design. The codebase half that holds them
  (`codebase_session_id` on `codebase_daemon_instance_id`) is never consulted.

## B. Why the two halves have different ids [V]: by design, not a linking bug

`split_claude_cli_start.rs:74` mints the agent id and `:81` mints a **second** id for the codebase
half before the peer is asked. That is the documented atomic-teardown guarantee
([remote-managed-worktree.md § Session start](../../ft/daemon/remote-managed-worktree.md#session-start),
"A knows the B-side session id before it asks", `requested_session_id`). Two `Uuid::now_v7()` calls a
few µs apart share the timestamp prefix, which is why only the last 12 hex digits differ. The link
is intact in both directions:

- Mac `.session.yaml`: `codebase_daemon_instance_id: mantas-udoo-ros`,
  `codebase_session_id: 01a1de51-…-b602f420269d`
- udoo `.session.yaml`: `session_type: workspace`, `agent_daemon_instance_id: LT-R1VXTH2V6H-…`,
  `agent_session_id: 01a1de51-…-b5fcbc66089c`

What is missing is any **consumer** of that link. Sections C and D both stem from tools and lists
treating the two halves as unrelated sessions.

## C. Why the inspection tools refuse the agent half [V]

Every project-home tool that targets another session goes through `resolve_target`
(`tddy-tool-engine/src/project_home_tools.rs:379-389`) → `resolve_session_target`
(`tddy-worktree-service/src/session_target.rs:68-89`). That function returns
`SessionTargetError::NoWorktree` (`:61`, `:86-89`) for any record without a worktree, **before** the
tool runs. Affected:

- `ReadSessionTerminal` (`project_home_terminal_tools.rs:224`), `ClaimSessionTerminal` (`:361`) and
  `SendSessionKeys`;
- `ReadSessionActivity` and `ReadSessionTranscript` (`session_follow.rs:309`);
- `SendSessionMessage` and `ResumeSession` (`project_home_turn_tools.rs:220`, `:244`);
- `AnswerSessionPermission` and `AnswerSessionQuestion` (`project_home_answer_tools.rs:227`, `:281`);
- `DeleteSession`, `SetSessionMark` and `RestoreWorktree` (`project_home_lifecycle_tools.rs:339`,
  `:294`, `:459`);
- `ReadSessionWorktree` and `DiffSessionWorktree` (`project_home_tools.rs:400`, `:477`).

Most of these never touch a worktree. The terminal, activity, transcript, turn, answer and
delete/terminate tools are keyed on the session id and the agent's host. The worktree is required
only because `SessionTarget.worktree` is a non-optional `PathBuf`.

What each should resolve through:

| Tool family | Should act on | How |
|---|---|---|
| terminal, activity, transcript, message, resume, answer, terminate, delete | the **agent** half (the Mac record) | let `SessionTarget.worktree` be `Option`; refuse only in the worktree tools |
| worktree read/diff, git ops (commit/push/rebase/stash/…) | the **codebase** half | when the record has `codebase_session_id`, retarget to (`codebase_daemon_instance_id`, `codebase_session_id`), the route `DiffSessionWorktree` on the udoo id already takes successfully |
| delete | both | the paired teardown already exists (`svc_paired_codebase_teardown`), once the gate lets it through |

`TargetRecord` (`session_target.rs:18-26`) does not carry `codebase_*` today, so the port's records
(`tddy-daemon-rpc/src/exec_tool/project_home/port.rs`) must add them. A related gap: asking the
**udoo** daemon for the Mac id answers `no session …` (home transcript 08:14:00), because only the agent
host knows that id.

## D. Why the web drawer "omits" the agent half [V code; I UI]

Nothing drops it. Traced end to end, read-only:

- **Server.** `ListSessions` (`session_coordinate_handlers.rs:46-94` → `session_reader::list_sessions_in_dir`,
  `tddy-session-activity/src/session_reader.rs:61-109`) returns every dir with a parseable
  `.session.yaml`. `repo_path: null` becomes `""` (`:93`). `ListProjectSessions` is served by the same
  reader, which is why the tool shows it.
- **Web.** `withoutJailedCodebaseHalves` (`jailedCodebaseHalf.ts:24-30`) drops only ids claimed as a
  codebase half. `groupSessionsByProject` keys on `projectId` alone. `projectHomeSlot`
  (`projectHomeGroups.ts:63-69`) removes only the home session itself.
- **Where it actually goes.** The dead agent has `is_active=false`, so `connectionStatusForSession`
  (`utils/connectionStatusForSession.ts:21-22`) answers `"disconnected"`, and
  `partitionSessionsByActivity` puts it in **Remaining** (`sessionStackGroups.ts:113-117`). When the
  project group also has an active row (the home session, the investigation sessions), Remaining is
  rendered with `defaultOpen={false}` (`SessionDrawerGroups.tsx:228-236`). The row is in the DOM but
  collapsed. [V code, I that this is what was seen: expanding "Remaining" in the tddy-coder group is
  the check.]

So D is a symptom of A: a living split agent would sort into **Active**. Two related defects:

- **The orchestrator link is dropped [V].** For any CLI-agent session type, list enrichment returns
  early with `orchestrator_session_id: String::new()` (`tddy-session-activity/src/session_list_enrichment.rs:192-203`),
  although the split agent's changeset names the home session. The drawer therefore cannot nest it
  under its orchestrator.
- **No pairing in the row.** The row shows the codebase-host badge (`SessionDrawerGroups.tsx:40-47`)
  but no branch, because the codebase half's branch is never joined onto the agent row.

## Proposed fixes, ranked (not implemented)

1. **Record and log a split agent's terminal and its exit (A.2).** Call `record_main_capture` from
   `spawn_split_agent` as the co-located and jailed paths do. Log every main's exit with status and
   lifetime in `spawn_terminal_cleanup` (`pty_spawn.rs:262-293`). Without this, every split failure stays
   undiagnosable. It is also the prerequisite for closing A's root cause. Small, and no behaviour risk.
2. **Fail a start whose agent does not come up (A.4).** After the spawn, wait for a liveness signal
   before reporting `succeeded`: the process alive after N seconds, or the MCP server's first
   connection. On failure, report the captured tail as the error and run the existing codebase
   teardown (`split_claude_cli_start.rs:171-200`). This turns a silent orphan pair into a failed start
   carrying the agent's own words.
3. **Load the split agent's hooks (finding 3, [V] by experiment).** With `--setting-sources user`, a
   hook in `.claude/settings.local.json` does not fire; with `user,project,local` it does. Either pass
   the hooks through `--settings <file>` (keeps `--setting-sources user` as the isolation from the
   checkout's `settings.json`), or allow `local` for the split variant (the context dir's
   `settings.local.json` is written by the daemon, not fetched from the checkout). Until then, no split
   session can report status, questions or notifications, even when its agent runs.
4. **Find and fix why Claude exits (A, open).** Do this after 1, from the captured output. Candidates,
   in order: the real `tddy-tools --mcp` with the LiveKit env (only part of the launch not reproduced
   offline); the daemon PTY's cleared env; the long multi-line prompt. Also add `run.env`
   (`CLAUDE_CODE_NO_FLICKER`) to the split env for parity (A.3).
5. **Make the inspection tools split-aware (C).** Make `SessionTarget.worktree` optional and refuse only
   in worktree tools. Carry `codebase_*` on `TargetRecord` and retarget the worktree/git tools to the
   codebase half.
6. **Report the codebase half's branch/worktree in the start result (A.4).** Look up
   (`codebase_daemon_instance_id`, `codebase_session_id`) in `start_ledger.rs:340` when the response's
   session is a split agent.
7. **Drawer (D).** Keep `orchestrator_session_id` for CLI sessions in `session_list_enrichment.rs:203`.
   Show the codebase half's branch on the agent row. Optionally render a dead split agent as "failed
   to start" rather than a plain disconnected row.
8. **Tests.** No CI test covers a cross-host split start end to end. Add one that asserts the agent
   process is alive and its MCP server connected after the start reports success, plus a unit test that
   the split argv loads the hooks file.

## Next step to close A

A read-only watcher (`pgrep -f -- "--setting-sources user"`; records full argv, env with secrets
redacted, parent, cwd and lifetime) was armed on the Mac for an hour from about 10:40 on
2026-11-15. One more split start in that window gives the exact argv and env to replay by hand,
which shows Claude's own exit message. Alternatively, land fix 1 first and read the recording.

## Cleanup list (nothing removed)

| Host | What |
|---|---|
| Mac | agent halves `01a1db70-…-b9c98689ad7c`, `01a1db9e-…-583311e9cae8`, `01a1de43-…-2ec505ca48ba`, `01a1de51-…-b5fcbc66089c` under `~/.tddy/sessions/` |
| udoo | codebase halves `01a1db70-…-b9dd0f328645`, `01a1db9e-…-58456f322ef2`, `01a1de43-…-2ed755ffc171`, `01a1de51-…-b602f420269d` under `/home/mantas/.tddy/sessions/` |
| udoo | worktrees and local branches `test-split-udoo`, `-2`, `-3`, `-4` under `/home/mantas/Code/tddy-coder/.worktrees/` |

Each agent half's `claude-mcp-config.json` holds a LiveKit and a session token until they expire (about
24 h).
