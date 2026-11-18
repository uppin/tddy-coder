# Initial discovery: target-agent-start

## Exploration 1 — whole-work discovery (copied)

**Date**: 2026-11-18 · **Base**: `feature/coder-start/sweep` (#944, ready) at `88a2495af`, on master `8b004d090`
**Temporary** — not a changeset companion; copied into each node's `{slug}-initial-discovery.md`, deleted at the pass-1 checkpoint (with consent).

## Source

The TODO gate of `#coder-start`'s sweep (2026-11-18): the developer chose **Continue** on all ten
todos the stack left open, one concern per node.

## Interview decisions (2026-11-18)

- Agent binary on a target: the spec's **bare command through the target's login shell**; `HOME`/`PATH`/`TMPDIR` from the target; only the spec's explicit agent env passed. Cursor's `create-chat` mint through the same target shell.
- Folder trust: **pre-trust the clone root** (and the project-MCP choice) in the target's `~/.claude.json` before the start.
- Streamed Coder commands: **OpenSSH with `ProxyCommand=coder ssh --stdio <ws>`** — a real pipe, no pty.
- One concern per node: start visibility and idle-as-permission are separate nodes; the live test and the fixture consolidation are separate nodes.
- `gh stack` #943: unstack and re-link as #944 + the new PRs (approved).

## Nodes (regrouped 2026-11-18: 5 nodes + sweep, approved)

| K | Node | Todo(s) | Wave |
|---|---|---|---|
| 5 | `coder-transport` | coder-one-shot-deadline-leaves-the-remote-process, coder-one-shots-start-pagers, coder-exec-streamed-crosses-the-pty-raw | 1 |
| 6 | `idle-status` | idle-session-reported-as-waiting-on-a-permission | 1 |
| 7 | `target-agent-start` | a-target-launch-runs-the-daemon-hosts-agent-path, cursor-chat-is-minted-on-the-daemon-host-for-a-target, claude-on-a-target-will-meet-the-folder-trust-prompt | 1 |
| 8 | `start-visibility` | coder-start-visibility-gaps | 2 |
| 9 | `target-test-kit` | 2026-11-16-remote-shell-test-fixtures-are-duplicated, 2026-11-16-coder-hosts-never-run-against-a-real-deployment | 3 |
| 10 | sweep | — | 4 |

## Conclusions

(filled after the explorations below)

## Exploration 1 — tddy-remote-shell (nodes 5 `coder-transport`)

Ref `origin/feature/coder-start/sweep`; `P` = `packages/tddy-remote-shell`.

**Budget.** `coder.rs` 405 production lines (`#[cfg(test)]` at 406), `coder/one_shot.rs` 107 — headroom 95 / 393.

**Deadline.** `ExecRequest.deadline` (`P/src/lib.rs:82`), default `DEFAULT_EXEC_DEADLINE` 60 s (`lib.rs:45`), `.within(d)` (`lib.rs:102`). `CoderShell::exec` (`coder.rs:308-364`) wraps the work in `tokio::time::timeout(deadline, work)` (351) → `RemoteShellError::TimedOut` (353-358). The only kill is `kill_on_drop(true)` in `CoderShell::command` (`coder.rs:194-214`, at 212): the local `coder ssh` dies, the remote `sh` is never signalled — the leak. `SshShell::exec` (`ssh.rs:133-198`, `kill_on_drop` at 116) has the same shape, without a pty.

**Wrapper.** `one_shot_command(command, stdin)` (`one_shot.rs:58-74`): no stdin → `stty -opost 2>/dev/null; exec </dev/null; {marker}; {command}`; with stdin → `stty -opost -echo …; {marker}; tddy_stdin=$(mktemp) … base64 -d … exec < "$tddy_stdin" …; {command}`. `{command}` is last in both, so `export PAGER=cat GIT_PAGER=cat GIT_TERMINAL_PROMPT=0;` and a remote bound (`timeout`) fit before it. `CoderShell::command` single-quotes it as one `sh -c` arg: `coder ssh --disable-autostart <ws> -- sh -c '<wrapped>'`. No test asserts the full wrapper string; acceptance tests check the `ssh --disable-autostart ada/devbox.main -- sh -c ` prefix (`tests/coder_shell_acceptance.rs:30,93`).

**Stand-in `coder`** (`P/tests/support/coder.rs`): `StandInCoder::running(body)` (107-133) writes a `#!/bin/sh` with a `TDDY_FIXTURE_WARMUP` guard; `RECORD_THE_CALL` (18-21) appends argv to `$here/calls` and the creds to `$here/environment`. Variants: runs-commands-here (30), merge-stderr (42), never-see-EOF (57, FIFO), CRLF until `-opost` (80, fake `stty` on PATH), refuses-to-connect (100). Nothing records a kill. Contract `a_command_past_its_deadline_is_timed_out` (`src/contract.rs:79-97`) asserts `TimedOut` only, not that the process died. A kill test: a stand-in body recording its command's pid / trap marker under `$here`, then `kill -0` after `TimedOut`. Pager test: `printf '%s|%s|%s' "$PAGER" "$GIT_PAGER" "$GIT_TERMINAL_PROMPT"` → `cat|cat|0`, no tty needed.

**Streamed.** `RemoteShell::exec_streamed(&self, &str) -> Result<StreamedExec, _>` (`lib.rs:178-179`); `StreamedExec { stdin, stdout, stderr, exit }` (`streamed.rs:22-27`). `CoderShell::exec_streamed` (`coder.rs:366-387`) runs the same `coder ssh … -- sh -c '<cmd>'` with no wrapper and hands over the raw pipes; `CoderStdin` (`coder.rs:86-123`) duplicates `SshStdin` (`ssh.rs:19-55`). `docs/architecture.md:69` documents the untranslated relay.

**SshShell spawn.** `SshShell::command` (`ssh.rs:106-118`): `ssh <ssh_options()> -- <alias> <cmd>`; `ssh_options()` (`ssh.rs:94-104`, `pub`, unit-pinned at 254) = `BatchMode=yes, ConnectTimeout=15, ServerAliveInterval=5, ServerAliveCountMax=3`; binary is the `ssh: PathBuf` field (bare `"ssh"` in `new`, `through(binary, alias)` overrides). A ProxyCommand route needs an ssh path on `CoderShell`, a host token (e.g. `coder.<owner>.<name>`), `-o ProxyCommand='<coder> ssh --stdio --disable-autostart <ws>'` (both shell-quoted), `StrictHostKeyChecking`/`UserKnownHostsFile` options. Creds stay env-only: the `ssh` child's env is inherited by the ProxyCommand.

**One ssh spawn site** (`P/tests/one_ssh_spawn_site.rs`) polices crates **other than** `tddy-remote-shell` — a spawn in `coder.rs` keeps it green.

**Contract.** `a_streamed_command_relays_until_it_exits` (`src/contract.rs:124-156`): `cat; printf done >&2; exit 4`, needs separate stderr, real EOF, exit status. Run for in-memory + `SshShell` (`tests/shell_contract.rs:16-33`) and `CoderShell` (`tests/coder_shell_acceptance.rs:59-67`) — the Coder run passes today only because the stand-in has no pty. `the_deployment_url_and_token_reach_only_the_coder_process` (`coder_shell_acceptance.rs:99-155`) counts exactly two credentialed `coder` calls (exec + exec_streamed) and forbids creds on argv — a ProxyCommand design must keep that true (the stand-in `ssh` has to exec the ProxyCommand so `coder` still records a `--stdio` call).

**Streamed callers.** `tddy-worktree-service/src/remote_git_service/child_relay.rs:87-91` (pack verbs) — only ever an `SshShell` today (65-70). `tddy-agent-launch/src/remote_hook_follower.rs:375` — the hook follower tails a log through `CoderShell` for Coder targets (built in `coder_target.rs:127`). So the real Coder streamed user is the **hook follower**. Other `CoderShell` constructor: `tddy-session-driver/src/target.rs:83`.

## Exploration 2 — tddy-agent-launch, agent-backend, test fixtures (nodes 7 `target-agent-start`, 9 `target-test-kit`)

Ref `origin/feature/coder-start/sweep`. `L` = `packages/tddy-agent-launch/tests`, `S` = `packages/tddy-session-lifecycle/tests`, `D` = `packages/tddy-session-driver/tests/session_driver_acceptance.rs`.

### Agent binary and env on a target

- `HostCodingAgents::resolve` (`tddy-agent-launch/src/spec_launch.rs:92-129`): no override → `registry.resolve_closure(&[id], self.host)` on the daemon host; override (`claude_cli.binary_path`) → `overridden_tool` (134-157, host `is_executable_file` + `canonicalize`). `ResolvedTool { command: <bare, e.g. claude>, binary: <absolute host path> }`; `HostAgent { spec, tool, deps, env_base }` (164-173).
- `{binary}` = `tool.binary`, substituted by `render_argv` (`tddy-actions/src/tool/template.rs:69,115-116`); `plan_host_pty` (`spec_launch.rs:~255-272`) renders `run.argv` and returns `run.env`. A target variant can render with `tool.command`. Cursor's `{node_launcher} {node_entry}` is computed on the host too (`opencode_cli_spawn.rs:312`, `node_launcher_of(&agent.tool.binary)`).
- `launch_on_target` (`remote_agent_launch.rs:163-221`) → `plan_the_launch` (233-250) → `write_rendered_session_files` (266-292) → `client_argv_and_env` (223-231) = `remote_command` (294-310) + `client_command` (312-344) → `spawn_rendered_main` → `judge_the_start` (346-393).
- **The leak:** `remote_command` calls `agent.env_base.agent_env(&agent.spec, std::env::vars_os(), &env)` — the daemon host's base set (HOME, PATH, TMPDIR…) — then `remote_agent_command(worktree, &plan.argv, &env)` (56-67, pub): `cd '<wt>' && mkdir -p .tddy && printf %s "$$" > .tddy/agent.pid && exec env -i K='V'… '<argv0>' …`, argv0 the host path. Pinned exactly by unit tests (456-540; the string at 471, env-only case at 525).
- `AgentEnvBase::agent_env(spec, parent, launch)` (`tddy-coding-agent-specs/src/agent_env_base.rs:120-134`): base-set names from `parent`, then `spec.passed_env(parent)`, then `launch`. An empty `parent` gives only the explicit env.
- `client_command` copies `SSH_AUTH_SOCK` and `PATH` for the **local** client — legitimate.
- `launch_on_target` callers: `opencode_cli_spawn/harness_launch.rs:167`, `tddy-session-driver/src/session.rs:111`. `RemoteLaunch` (71-88): `shell, worktree, session_dir, pointer, startup_window` — no home/tmp.
- Existing model for reading the target's environment: `remote_checkout_root` (`tddy-git/src/remote_checkout.rs:149`), a sentinel probe (`TDDY_REMOTE_HOME_BEGIN:…`) that tolerates pty noise.
- Specs (`tddy-coding-agent-specs/specs/`): claude `binary: {command: claude, env: TDDY_CLAUDE_BINARY}`, `run.argv[0] "{binary}"`, `run.env {CLAUDE_CODE_NO_FLICKER: "1"}`, `pass_env [ANTHROPIC_API_KEY]`; cursor `binary: {command: agent, env: TDDY_CURSOR_AGENT}`, argv0 `node_launcher` or `{binary}`, `pass_env [CURSOR_API_KEY]`, `conversation: {kind: minted, adapter: cursor-create-chat}`.

### Cursor mint

- `start_conversation` (`tddy-agent-launch/src/cli_agent_steps/conversation.rs:82-125`), input `ConversationStart { spec, binary, worktree, env_base, env, session_id }` (64-78) — no shell. Minted → `ConversationMint { binary, workdir: worktree, env: agent_env(spec, std::env::vars_os(), env), deadline }` (113-119).
- Callers: `opencode_cli_spawn.rs:264` (`target_shell` in scope, l.112), `opencode_cli_spawn/resume.rs:268` (`target_shell` l.56/211), `svc_start_sandboxed_cli_harness_session.rs:294` (jail, no target); tests `cli_agent_steps/cli_agent_steps_tests.rs:96-197`.
- `CursorCreateChat::mint` (`tddy-agent-backend/src/adapters/conversation_id.rs:91-120`): `Command::new(mint.binary).arg("create-chat").current_dir(mint.workdir)` + `apply_agent_child_env` + `timeout` → `parse_created_chat_id`. Trait `ConversationIdAdapter::mint(&self, ConversationMint)` (73-78); `ConversationMint` (36-48) is `Copy`. A target mint needs the shell in `ConversationStart` and a shell-run path (`cd <wt> && env -i … agent create-chat` through `shell.exec`), parsed tolerantly of pty noise.

### Folder trust

- **No code edits `~/.claude.json`** anywhere (grep `claude.json`, `hasTrustDialogAccepted`, `enabledMcpjsonServers`, `enableAllProjectMcpServers`).
- Target writes today: `write_slot_files` (`remote_agent_launch.rs:252-264`), `write_rendered_session_files` (266-292), `write_slot_files_through_shell` (`cli_agent_steps.rs`; called `opencode_cli_spawn.rs:296-297`, `resume.rs:297`) — all `shell.write_file(path, bytes, FileMode::TargetDefault)`; `write_file` = `mkdir -p && cat >`, `read_file` = `cat`; `FileMode::OwnerOnly` = `umask 077`. No atomic-rename helper. `$HOME` on the target is not known to `launch_on_target`.
- Clone root: `tddy-git/src/remote_checkout.rs` — `REMOTE_REPOS_DIR = ".tddy/repos"` (22), `RemoteCheckout { root: <home>/.tddy/repos/<repo>, worktree: <root>/.worktrees/<session>, branch }` (46-52), cut at 170-185; called from `tddy-agent-launch/src/cli_session_worktree.rs`. `launch_on_target` sees only `remote.worktree`; the root is its `.worktrees` parent, or pass it in `RemoteLaunch`.

### Duplicated target-test fixtures

- `tddy-testing-commons` exists (`assertions, builders, dependency_graph, fakes, fs, gh_cli, process_env, sandbox_fixtures, sandbox_session_channel, stub_http, stub_http_routed, stub_scripts, wait`); `stub_scripts` has `StubAgentScript::{recording_argv_to, recording_env_to, appending_argv_to}`. No `write_executable`, warm-up, ssh/coder stand-in or `acme` spec. Dev-dep of `tddy-agent-launch` (Cargo.toml:65) and `tddy-session-lifecycle` (:129), **not** `tddy-session-driver`.
- `L/support/` = real-sshd askpass fixtures only; `S/support/` = tmux tab fixtures (+ several `*_support` dirs). Session-driver has none.
- **`write_executable` + `TDDY_FIXTURE_WARMUP` guard** — 8 byte-identical copies (`L/remote_agent_launch_acceptance.rs:71-94`, `L/remote_hooks_acceptance.rs:53-76`, `L/coder_agent_launch_acceptance.rs:72-95`, `S/coder_target_session_acceptance.rs:62-85`, `S/remote_target_session_acceptance.rs:53-76`, `S/remote_hooks_session_acceptance.rs:49-72`, `S/derived_host_start_acceptance.rs:59-82`, `S/orchestrator_target_start_acceptance.rs:68-91`), 2 diverged (`L/coder_agent_launch_on_path_acceptance.rs:55-76`, `D:66-92`), 1 shell-embedded (`S/agent_registry_support/mod.rs:314,567`).
- **Stand-in ssh target** — `ATarget` (`S/remote_target_session_acceptance.rs:84-156`, `S/remote_hooks_session_acceptance.rs:80-152`), `AReachableMachine` (`S/derived_host_start_acceptance.rs:90-156`, `S/orchestrator_target_start_acceptance.rs:99-165`, identical pair); agent-launch `ATarget` (`L/remote_hooks_acceptance.rs:84-155`; `L/remote_agent_launch_acceptance.rs:102-225`, the most evolved: call log, `LOGIN_ONLY` login env, pointer/pid files).
- **Stand-in coder** — `AWorkspace` (`S/coder_target_session_acceptance.rs:93-172`, `L/coder_agent_launch_acceptance.rs:159-274` + `AVault`, `ACoderCall`), `AMachine` in `D:92-173` (diverged, no HUP trap).
- **Stub agents** — lifecycle `a_stub_agent(runs)` in 4 files (variants: args logged, USR1 trap) + `an_agent_that_fires_its_start_hook`; agent-launch `AStubAgent` in 3 files; `D:65` `AN_AGENT_THAT_EXITS`; `S/agent_registry_support` has a separate larger family.
- **`acme` spec/agent** — `the_acme_spec` identical in `L/remote_hooks_acceptance.rs:196`, `L/coder_agent_launch_acceptance.rs:343`; `the_acme_agent` in 4 files (2 diverged); `D` `ACME_TOOL_FILE`; constants (`acme-cli`, `acme-1`, `acme`, `acme-work`, `buildbox`) repeated everywhere.
- **Daemon helpers** — `ADaemon`/`a_daemon` in 3 agent-launch files; `ADaemonWithATarget`/`ADaemonWithAWorkspace`/`ADaemonReachingAnSshHost` in 5 lifecycle files, near-identical.
- Sharing route: a `tddy-testing-commons` module for `write_executable` + warm-up and stand-in ssh/coder builders (script body + root mapping); the `acme` `HostAgent` helper cannot go to testing-commons if that would cycle (`HostAgent` is `tddy-agent-launch`'s) — a `tests/support/` module per crate for that part.

## Exploration 3 — start ledger, pending questions, session driver (nodes 6 `idle-status`, 8 `start-visibility`, 9 `target-test-kit`)

Ref `origin/feature/coder-start/sweep`.

### Start visibility

- **`GetSessionStart` is an exec tool returning JSON, not a proto RPC.** Record and names: `packages/tddy-tool-engine/src/start_ledger.rs` (consts 33-34); handler `project_home_tools.rs:310` (dispatch :216, port trait :119); daemon impl `packages/tddy-daemon-rpc/src/exec_tool/project_home/port.rs:137`; catalogue text `project_home_catalogue.rs:216`; read-only allow-list `read_only.rs:107`.
- `StartLedgerRecord` (`start_ledger.rs:77-97`): `start_id, task_id, daemon_instance_id, state, step {step,status}, clone_progress, session_id, branch, worktree, error, updated_at_unix_ms, display_name`. JSON at `<orchestrator session>/artifacts/session-starts/<start_id>.json` (132-136), atomic `persist` (221), `recover_interrupted` (178). One record, **no list field**.
- Steps: `begin_recorded_start_with` (247) → `tokio::spawn(record_until_end)` (283, 305) → `apply_progress` (399): `Phase` events via `step_of` (419) from proto `StartPhase` (`packages/tddy-service/proto/session.proto:111`, enum `Step` :112; `StartSessionEvent` oneof ~532-534) → `clone | worktree | semantic_index | agent`; `CloneProgress` events set `clone_progress`. Producers: `AttachmentProgressSink::begin_phase/end_phase` (`packages/tddy-session-files/src/attachment_progress.rs`; e.g. `cli_session_worktree.rs:285`).
- **Null branch/worktree on failure:** only `Ending::Succeeded` (293) carries them, looked up once via `StartedSessionDetails::listing` (112, `ending_of_result` 354; impl `StartedSessionListings`, `tddy-agent-launch/src/conversation_start.rs:186`, wired 71-76). `Failed`/`Cancelled` (386-394) set only `error`. The remote cut happens in `remote_worktree_through` (`cli_session_worktree.rs:181-249`) returning `RemoteCheckout {root, worktree, branch}` (`tddy-git/src/remote_checkout.rs:185`) — never reported to the ledger.
- **Remote commands of a start:** `setup_remote_worktree` (`remote_checkout.rs:172-232`): home probe (149), `test -d <root>/.git` → clone / init (`ensure_the_clone` 237), `test -d <worktree>`, `fetch --all`, `worktree add [-b]`; errors → `RemoteCheckoutError` (58) → `Status::internal("remote worktree setup failed: …")` (`cli_session_worktree.rs:247`). Other start-time execs: `cli_session_pointer.rs:237,296`, `remote_agent_launch.rs:176`, `remote_agent_launch/startup.rs:37,87`, `remote_hook_follower.rs:290,308`, `slot_files_over_shell.rs:49`.
- **Choke point:** the `RemoteShell` trait (`tddy-remote-shell/src/lib.rs:176,179`, + `interactive`); impls `SshShell::exec` (`ssh.rs:133`), `CoderShell::exec` (`coder.rs:308`), `InMemoryShell` (`in_memory.rs:200`, `test-support`). A recording decorator sees `address()`, the command string, duration, exit, stderr, `Err`. `ExecRequest {command, stdin, deadline}` (`lib.rs:75`) — no argv/cwd split. The start's shell is built at `cli_session_worktree.rs:143` (`start.shell`) / `:174` (`SshShell::new(start.ssh_alias)`).
- **"no answer within 60s"** = `RemoteShellError::TimedOut {address, after}` Display (`tddy-remote-shell/src/error.rs:35-38`), raised at `ssh.rs:175-182`, `coder.rs:354`. **The command is not in the error.** Related: `cli_session_worktree.rs:240-246` (`start.timeout` 600 s, `config.rs:563`), `start_deadline.rs:191`.
- **`usable`** (`DescribeSessionStartOptions`, exec tool; `tddy-tool-engine/src/start_options.rs:76-78`, `tddy-daemon-rpc/src/exec_tool/project_home_start_options.rs:291-330`): copied from `ListDerivedHosts`; `host.proto:484-490` — SSH always usable; Coder usable iff `running` and the daemon has the `coder` CLI (`tddy-host-service/src/coder_derived_hosts.rs:173-182`). Never a probe.
- Proto changes only if new `StartSessionEvent`/`StartPhase` data: `session.proto` + committed TS (`packages/tddy-client-web/src/gen/session_pb.ts`, `packages/tddy-rust-typescript-tests/gen/session_pb.ts`). No proto manifest. Tests calling `GetSessionStart`: `tddy-daemon-rpc/tests/{async_start_acceptance.rs:138,297, session_display_name_acceptance.rs:210, spawn_capable_session_tools_acceptance.rs, support/mod.rs:502}`, `tddy-tool-engine/tests/session_display_name_reports.rs:97`. Doc: `docs/ft/daemon/project-concept.md` § Project-home tools.

### Idle reported as a permission wait

- Hooks only, no scraping. Remote: `remote_hook_command` (`tddy-agent-backend/src/claude_hooks.rs:55`) → `RemoteHookFollower` → `ActivityHookSink::deliver` (`remote_hook_follower.rs:~187`) → `tddy_session_activity::hook_request::session_status_request`; local: `tddy-tools session-hook`, same function. `CLAUDE_HOOK_EVENTS` (`claude_hooks.rs:78`).
- Mapping (`tddy-changeset/src/session_activity.rs:109-133`): a `Notification` with `permission_prompt | elicitation_dialog | idle_prompt | question_prompt` → `WaitingForInput`. `hook_request.rs:54-67,74-75,86`: `tool_name` only for question-asking events, else empty.
- Daemon: `report_session_status_impl` (`tddy-session-activity/src/service/hook_reports.rs:171`); `question_asked_by` (90) mints a `request_id` only with `WaitingForInput` **and** a tool; `record_pending_question` (118-155).
- **The defect:** `hook_reports.rs:140-149` — `WaitingForInput`, no tool, no kept structured question, non-empty `message` → `PendingQuestion { kind: "permission", question: message, ..Default::default() }` (`request_id ""`), `set_pending_elicitation(true)`, `PendingChange::Plain`. Comment: preserves `#start-parity` 6/9 behaviour. That is Claude's `idle_prompt` Notification. `PendingQuestion` (`tddy-changeset/src/pending_question.rs:29`; `request_id` `#[serde(default)]`, doc 40-41 "empty for a bare Notification").
- **Refusal:** `refuse_while_waiting_on_an_answer` (`tddy-daemon-rpc/src/exec_tool/project_home/turn.rs:133-156`) refuses on any `pending-question.json`, regardless of `request_id`/status → `waiting on permission ""`. Also `watch_wake.rs:290-303` and `tddy-agent-launch/src/project_home_idle.rs` treat any pending question as non-idle — the bare record suppresses idle wakes too.
- Tests: `tddy-session-activity/tests/pending_question_capture_acceptance.rs` (:431, :507, :540, :557), `permission_answer_acceptance.rs`, `question_answer_acceptance.rs`, `pending_question_record.rs:123-220`; `tddy-daemon-rpc/tests/project_home_turn_control_acceptance.rs:358`; `tddy-changeset/tests/pending_question*.rs`. **No test feeds an `idle_prompt` Notification through `report_session_status`.**

### Live test via the session driver

- `tddy-session-driver` args: `--target local | ssh:<alias> | coder:<owner>/<ws>[.<agent>]`, `--worktree`, `--agent`, `--model`, `--prompt`, `--permission-mode`, `--binary`, `--env K=V`, `--tddy-home`, `--coder`, `--startup-window-secs` (5), `--rows`/`--cols`, `--start`; Coder needs `CODER_URL`/`CODER_SESSION_TOKEN`. MCP tools: `start_session, read_terminal, send_keys, wait_for_text, session_status, stop_session`. 7 acceptance tests, stand-ins only.
- Gating convention (`docs/dev/guides/testing.md:317-354`, `:34`, `:150-160`): production tests are `#[ignore = "production test: … run with --ignored"]`; a missing env var is a **failure**, never a skip (`tddy_vm_testkit::require_env_path`, `tddy-vm-testkit/src/env_file.rs:102`; `.env` read). `./vm-tests` lists suites explicitly. No Coder-live test exists.
- Facts still unverified (todo 2026-11-16): `classify_coder_version` (`tddy-host-service/src/host_tooling.rs:113-128`, tests 922-950: second word of a `Coder …` first line, `+build` stripped); `CoderWorkspace::is_running` (`coder_workspaces.rs:83-85`, `== "running"`); the hook follower (`remote_hook_follower.rs`, one-way, `exec_streamed` tail; `RemoteHookFollower::start` 326) on a real workspace.

## Exploration 2 — node-specific

This node's State A is the matching section above; pass 2 appends what the contract needed.
