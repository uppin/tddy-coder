# 2026-11-19 — A target start hands the remote side paths of the daemon's host

**Category:** Defect (blocks every Claude start on a Coder workspace or a non-mirrored SSH host)
**Source:** investigation of a failed start on `mantasi/remote-dev` (Coder v2.33.5, Linux,
`HOME=/home/coder`, uid 1001 `coder`, no root), orchestrator session `01a17692-…`, start record
`~/.tddy/sessions/01a17692-…/artifacts/session-starts/start-1795066655480-49443-2.json`, child
session `01a1f24f-7302-7521-b951-e380ae501dd5`. Source read at `a4ab9db5b` (#953). `origin/master`
has not moved since, so every line reference below is current. No code was changed.

**Narrowed 2026-11-19 (this branch, `investigate/coder-remote-agent-local-paths`):** fixes 1 (the
argv-path part only), 2 and 3 of §6 landed — see
[`docs/dev/changesets/2026-11-19-target-start-host-paths.md`](../changesets/2026-11-19-target-start-host-paths.md).
L1/L2, §3 and §4 are closed. **Still open:** L3/L4 (the notify wiring on a target, which needs the
decision in §2), L5 (usage `agent_home`), L6/L7 (a recipe on a target is not refused), L8 (a route
config file named by an env value), and fixes 4–5. The live check under *What would close this file*
has not been run.

The earlier diagnosis holds. This file confirms it against the source, fills the gaps it left open,
and ranks the fixes.

## TL;DR

1. **Root cause (confirmed):** `launch_on_target` copies every argv path under the *local* session
   directory to the **same local path** on the target
   (`packages/tddy-agent-launch/src/remote_agent_launch.rs:347-372`). For a Claude start that path is
   `--append-system-prompt-file /Users/mantasi/.tddy/sessions/<id>/agent-system-prompt.md`, so the
   target runs `mkdir -p '/Users/…'`, which fails on Linux. The behaviour dates from
   #917 (`51fab2980`, `#remote-shell` 3/8). Its tests pass because the stand-in target shares the
   test host's filesystem, so a host path is writable "on the target" too.
2. **Opaque error (confirmed live):** on Coder the command's stderr arrives on **stdout**, because
   the pty merges the two streams. `ExecOutput.stderr` then holds only the `coder` CLI's own wrapper
   text, and that is all `stderr_tail` and `RemoteShellError::Exited` report.
3. **Stale base (confirmed):** a remote `new_branch_from_base` start with no explicit or chain base
   cuts from the literal `'HEAD'` (`packages/tddy-agent-launch/src/cli_session_worktree.rs:292-297`).
   That is the clone's local `master`, which `fetch --all` never moves.
4. **Daemon reach:** hooks reach the daemon by a built path (an append-only log followed over the
   shell). `tddy-tools` has **no path at all**: the target has no tddy software by design. The local
   `TDDY_DAEMON_SOCKET` in the target's `.tddy/session.json` is therefore a **bug of omission**. The
   start filters the session tool transport for a target but not the operator-notify wiring.
5. **No open PR covers any of this.** #939–#949 and #952 are all merged. No `docs/dev/todo/` entry
   names these defects.

## 1. Every host path that reaches the remote side of a target launch

The code path is the same for both kinds of target: `start_cli_harness_session`
(`packages/tddy-agent-launch/src/opencode_cli_spawn.rs:57-…`) → `harness_launch::launch`
(`opencode_cli_spawn/harness_launch.rs:167`) → `launch_on_target`
(`remote_agent_launch.rs:215`). The two targets differ only in the shell: `SshShell` or
`CoderShell` (`opencode_cli_spawn/session_start.rs:94-110`).

### What actually crossed in the failed start, read back from the workspace (read-only)

| Remote artefact | Content | Host path? |
|---|---|---|
| `.claude/settings.local.json` | the remote hook carrier only: `printf … >> '.tddy/hooks.log'`, relative path | **clean** |
| `.tddy/hooks.log` | empty, created by `create_remote_hook_log` (`remote_hook_follower.rs:277`) | clean |
| `.tddy/session.json` | `repo_dir` remote; `notify.TDDY_DAEMON_SOCKET=/var/folders/…/tddy-agent-tools-49443-0.sock`, `TDDY_OS_USER=mantasi`, `TDDY_HOOK_TOKEN`, `TDDY_SESSION_ID`; `owner {pid 49443, host LT-R1VXTH2V6H}` | **socket leaks** |
| `mkdir -p '/Users/mantasi/.tddy/sessions/01a1f24f-…' && cat > '…/agent-system-prompt.md'` | the failing call | **leaks** |
| `command -v tddy-tools` on the workspace | nothing. Only `/home/coder/.local/bin/claude` exists | — |

### Every leak, including the latent ones this start never reached

| # | What | Where it is built | Reaches the target as | Remote equivalent |
|---|---|---|---|---|
| L1 | **Appended system prompt** (the notify guidance, plus a recipe prompt when there is one) | written to `<local session_dir>/agent-system-prompt.md`, `notify_env.rs:27,106-112`; reaches the argv through `system_prompt_file`, `opencode_cli_spawn.rs:292` | argv `--append-system-prompt-file /Users/…/agent-system-prompt.md`, then copied to that same path, `remote_agent_launch.rs:249,354,366-369` | a target-side session directory, e.g. `<worktree>/.tddy/agent-system-prompt.md`. `.tddy/` is already 0700 there and holds `session.json`/`hooks.log`. The argv entry is rewritten to that path. |
| L2 | **The copy rule itself**: "a file under `remote.session_dir` is written on the target at the same path" | `remote_agent_launch.rs:244-249` (comment) and `:347-372`. `RemoteLaunch.session_dir` is documented as *this host's* directory, `:107-109` | any argv file under the local session dir | a local→remote **path map**: the argv is rewritten and each file is written at its mapped path. |
| L3 | **`TDDY_DAEMON_SOCKET`** (the agent-tool socket of the session's OS user, under macOS `$TMPDIR`) | `notify_env_pairs(..., Some(agent_tool_socket))`, `notify_env.rs:104`; placed in the pointer by `session_pointer_of`, `cli_session_pointer.rs:38-66` | `.tddy/session.json` `notify` on the target, through `place_slot_files_and_pointer` (`opencode_cli_spawn.rs:300,440-459`) | **none exists.** No tddy software is on the target. See §2. |
| L4 | `TDDY_OS_USER=mantasi`, `owner {pid, host}` of the daemon | `notify_env.rs:52-54`, `cli_session_pointer.rs:65` | `session.json` | `TDDY_OS_USER` is the daemon-side owner, an identity rather than a path, and harmless alone. `owner` is rewritten to the remote agent by `record_remote_owner` once the agent starts (`cli_session_pointer.rs:199`), so it stayed local here only because the start failed first. |
| L5 | **Agent home in usage context** | `record_usage_context(..., home_dir_for_user(os_user), …)`, `opencode_cli_spawn.rs:322-328` (also on resume, `opencode_cli_spawn/resume.rs`) | `usage-context.json` on this host: `agent_home: "/Users/mantasi"` beside `worktree: "/home/coder/…"` | the target user's home (`/home/coder`), which the checkout already probes (`remote_checkout_root`, `tddy-git/src/remote_checkout.rs:146-…`), **and** a record that the files are on a target. `tddy-usage` reads `{agent_home}/.claude/projects/*/{conversation_id}.jsonl` on this host (`claude.yaml:73`), so it finds nothing for a target session even with the right home. Reading usage over the shell is a separate piece of work. |
| L6 | **MCP registration file** (latent): `claude-mcp-config.json` in the session dir, naming the **host's** `tddy-tools` (`agent_tddy_tools`, `cli_harness_jail.rs:39-42`) and `TDDY_SOCKET`/`TDDY_SESSION_DIR` | `carry_on_the_tool_server`, `cli_agent_steps.rs:445-472` (`command: PathBuf::from(tddy_tools_path)`, `dir: session_dir`); `host_launch_slots.rs:48-51` | a slot file at a host path, written through the shell at that host path (`write_slot_files_through_shell`, `host_launch_slots.rs:92-…`), so it fails the same way as L1. The argv then gets `--mcp-config /Users/…` | none: no `tddy-tools` on the target. Only a recipe (managed workflow) or a role produces this file. |
| L7 | **Managed workflow on a target** (latent): `TDDY_SOCKET` (host Unix socket), `TDDY_SESSION_DIR`, the recipe prompt file in the session dir | `managed_launch::prepare_managed_workflow_inner`, called at `cli_agent_steps.rs:148-166` | L1 + L6 | not supported. **`refuse_a_target_combination` does not refuse a recipe** (`start_request_checks.rs:67-112` checks managed_codebase, sandbox, sandboxed_codebase, codebase daemon, split, ssh_config_host only). Today a recipe on a target fails late, with an L1-style error. |
| L8 | **Route config file** (latent, specs with `route.config_env`, e.g. OpenCode) | `cli_route_launch.rs:91-107`: the file is written into the local session dir and `<CONFIG_ENV>=<session_dir>/<file>` is put into the env | an **env** value, which `write_rendered_session_files` never scans (it reads argv only), so the agent starts and reads a missing file | the file is written owner-only on the target (it can hold a key) and the env value is mapped. Claude's route is env-only (`claude.yaml:56-58`), so Claude is unaffected. |
| L9 | The spec's `pass_env` values, read from the **daemon's** environment | `remote_agent_launch.rs:388` | the env inside the remote command | by design (`ANTHROPIC_API_KEY` for Claude). Not a path, but the same rule should apply: a host path in a passed value is wrong on a target. |
| — | Not leaks, for completeness: the local PTY's cwd is the local session dir (`remote_agent_launch.rs:259`), which is correct because the PTY runs `coder`/`ssh` here. The agent binary is the bare command (`:324`). `HOME`/`PATH`/`TMPDIR` are the target's (`:48`). Slot files under the worktree (`.claude/settings.local.json`) are remote paths. The session tool transport is filtered for targets (`opencode_cli_spawn.rs:239-242`). The hooks are the remote carrier (`:245-247,427-438`). | | | |

**SSH targets are affected the same way.** The code is shared. An SSH host "works" only when the
daemon user's local session path happens to be creatable there (for example another Mac with the
same user name). A Linux SSH host with a different home fails exactly as Coder does, though with a
readable error (see §3).

## 2. How remote `tddy-tools` and hooks are meant to reach the daemon

- **Hooks: built.** On a target, `log_hooks_on_the_target` sets `remote_log`
  (`opencode_cli_spawn.rs:427-438`), so each hook is a `printf` append to `.tddy/hooks.log`
  (`tddy_core::remote_hook_command`). The daemon tails that file over the session's shell
  (`remote_hook_follower.rs`, started at `harness_launch.rs:192-211`) into the same
  `ReportSessionStatus`/`ReportAgentActivity` handling a local hook reaches. Hooks that wait for an
  answer are not installed. See `docs/ft/daemon/claude-cli-session.md` §"On a target session" and
  `docs/ft/daemon/changelog/2026-11-16-remote-shell-remote-hooks.md`. **Still unverified live:** the
  follower on a real Coder workspace (`docs/dev/todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md`).
- **`tddy-tools` from the agent's shell (`notify`, `submit`, `ask`, `monitor`, `send`): nothing,
  by design.** The feature docs say a target "needs no tddy software"
  (`claude-cli-session.md:569-571`, `remote_agent_launch.rs:1-2`). Nothing reaches the daemon from
  the target: no socket forwarded over the transport (`ssh -R`/`coder port-forward`), no relay,
  no LiveKit.
- **So the local `TDDY_DAEMON_SOCKET` is a bug, not an unbuilt piece.** The start already applies
  this reasoning to the session tool transport. It withholds it from a target because "the URL and
  the socket name this daemon's host, which that shell cannot reach"
  (`opencode_cli_spawn.rs:239-242`, `claude-cli-session.md:598-600`). The operator-notify wiring was
  missed: `wire_session_agent` runs `agent_notify_unless_role` whatever the target
  (`cli_agent_steps.rs:170-183`). As a result a target session gets:
  - a pointer naming an unreachable socket (L3), which also places the session's hook token in a
    file on the target that nothing there can use;
  - **the notify guidance in its system prompt**, telling the agent to run a `tddy-tools notify`
    that does not exist on the target. L1 is the vehicle of this mistake, not only a wrong path.
- **If notify on a target is wanted later (an unbuilt piece, needs a product decision):** the
  cheapest carrier is the one hooks already use. Give the agent a `printf`-only notify command that
  appends a `Notify` line to `.tddy/hooks.log`, and let the follower deliver it. This keeps the
  promise of no tddy software. A forwarded socket would need `tddy-tools` on the target and a
  per-transport reverse tunnel.

## 3. Why the remote command's real stderr is lost

Reproduced live (read-only, `mkdir -p /Users/x` in the one-shot shape):

```
exit=1
--stdout   MARKER\r\n mkdir: cannot create directory '/Users': Permission denied
--stderr   Encountered an error running "coder ssh" … error: run command: Process exited with status 1
```

- `coder ssh` always runs the command in a pty, so the command's stderr **and** the start marker
  arrive on the CLI's **stdout**. The CLI's own stderr carries only its wrapper text
  (measured 2026-11-18; `one_shot.rs:57-86`).
- `CoderShell::output_of` (`packages/tddy-remote-shell/src/coder.rs:315-351`) finds the marker on
  stdout (`:328-334`). It returns `stdout` = the merged command output (where "Permission denied"
  is) and `stderr` = the CLI's wrapper text.
- `ExecOutput::success_or_exited` (`tddy-remote-shell/src/lib.rs:143-152`) builds
  `RemoteShellError::Exited { stderr }` from `stderr` alone. `write_file` (`lib.rs:201-224`) uses it,
  so the start error became "the command exited with status 1: Encountered an error running
  "coder ssh"…".
- `RecordedShell::exec` (`tddy-remote-shell/src/recorded.rs:69-78`) fills `stderr_tail` from
  `output.stderr` only (`stderr_of`, `:114`). That is what the start record shows. The merged stdout
  is captured in `ExecOutput.stdout` and then dropped by both.
- The checkout code has the same blind spot. `target_words` (`tddy-git/src/remote_checkout.rs:117-125`)
  falls back to stdout only when stderr is empty, and on Coder stderr is never empty for a failure.
- SSH is not affected: `SshShell::exec` runs without a tty, so its stderr is separate.
- The daemon log shows nothing for this start because the failure travels only as the start's
  `Status`. `report_the_started_session` (`opencode_cli_spawn.rs:472-…`) logs successes, and nothing
  logs the failure path.

## 4. Where the remote worktree base is chosen, and why it is stale

- `cut_worktree` (`cli_session_worktree.rs:142-175`) picks
  `select_worktree_base_ref(selected_integration_base_ref, chain_base_ref)`
  (`tddy-session-worktree/src/session_chain.rs:272-282`). That is `None` for this start: the
  changeset records `new_branch_from_base` and no base.
- **The local cut** passes `None` to `setup_worktree_for_session_with_optional_chain_base`, which
  resolves a real default: `resolve_default_integration_base_ref` → `git fetch <remote>`, then
  `origin/master`, then `origin/main`, then the remote's HEAD
  (`tddy-git/src/remote.rs:218-281`). #951 made the project's main ref runtime-resolved for its other
  consumers (`effective_integration_base_ref_for_project`,
  `tddy-projects/src/project_storage.rs:312`).
- **The remote cut** (`remote_worktree_through`) turns `None` into the literal `"HEAD"`
  (`cli_session_worktree.rs:292-297`). `setup_remote_worktree` then runs `git fetch --all`
  (`remote_checkout.rs:202-210`) and `worktree add -b <name> <path> 'HEAD'` (`:212-219`).
- **Why it is stale:** `fetch --all` updates only remote-tracking refs (`refs/remotes/origin/*`).
  Nothing updates `refs/heads/master`, and the clone's `HEAD` stays on the master it was cloned or
  last pulled at. On the workspace: `HEAD`=`master`=`9763fd02c7`, `origin/master`=`5102a5ded4`. The
  `RemoteBranch::NewFrom` doc already expects "a remote-tracking ref such as `origin/main`"
  (`remote_checkout.rs:26-27`). The caller never supplies one.
- **What the base should be:** the same ref the local cut would choose, evaluated **on the clone that
  is cut**: `<remote>/<main>` after the fetch. There are two ways to get it:
  (a) take the project's effective integration base ref, already resolved on this host (e.g.
  `origin/master`), and pass it as `base_ref`. This is cheap, but it assumes the remote clone has the
  same remote name.
  (b) resolve it on the target after the fetch: `git -C <root> symbolic-ref --short
  refs/remotes/origin/HEAD`, falling back to `origin/master`/`origin/main` as
  `resolve_default_integration_base_ref_with_remote` does. That costs one more 6–8 s Coder call.
  **Recommend (a)**, with an existence check folded into the `worktree add` command so that no extra
  round trip is spent.

## 5. What is already covered

| Item | Open PR? | TODO? |
|---|---|---|
| L1/L2 host-path copy | none (#939–#949, #952 merged; #950 no longer exists) | none |
| L3 notify socket / guidance on a target | none | none |
| L5 usage `agent_home` | none | none |
| L6/L7 recipe or MCP file on a target | none | none (and no refusal) |
| L8 route config env | none | none |
| §3 Coder stderr | none. #948 (`start-visibility`, merged) added the record and `stderr_tail` but took stderr as the CLI reports it | none |
| §4 stale `'HEAD'` base | none. `#remote-shell` 10/11 (#926) made the cut honour the branch intent but left the `"HEAD"` default | none |
| Host must also have the agent binary | — | `2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md` (related: a target start still resolves host paths first) |
| Hook follower unverified on real Coder | — | `2026-11-16-coder-hosts-never-run-against-a-real-deployment.md` (still open; this start never reached the agent) |
| Why the tests missed it | — | `2026-11-16-remote-shell-test-fixtures-are-duplicated.md` (related: the stand-in target shares the host's filesystem) |

## 6. Proposed fixes, ranked

All of these need **new PRs**: there is no open PR in this area to fold them into.

1. **P0. A target launch names no host path** — *the argv map landed (2026-11-19); the notify, recipe and env bullets remain* (`tddy-agent-launch`), one PR:
   - Replace the "same path" rule (`remote_agent_launch.rs:347-372`) with a **session-dir map**:
     local `session_dir` → `<worktree>/.tddy/` on the target. Rewrite every argv entry (and every
     env value, which covers L8) that names a mapped file. Write each file at its mapped path, owner-only.
   - Withhold the operator-notify wiring for a target, as the session tool transport already is
     (`cli_agent_steps.rs:170-183`, gated like `opencode_cli_spawn.rs:242`). That means no guidance in
     the prompt and no `notify` in the pointer, which removes L3 and the hook token from the target.
     If no recipe or role prompt remains, no `--append-system-prompt-file` is rendered at all, which
     removes L1 for a plain Claude start on its own.
   - Refuse a recipe (managed workflow) on a target in `refuse_a_target_combination`, with the
     reason, until a remote tool path exists. This covers L6/L7.
   - Guard test: a plan for a target start, rendered against a session dir that the stand-in target
     **cannot** write (a path outside the stand-in's root), carries no host path in argv, env or
     pointer. This is the test that would have caught #917.
   - Decision for the user: confirm "no notify on targets" now, or the hooks-log notify carrier
     (§2) as a follow-up.
2. ~~**P1. Coder failures say why**~~ — *landed 2026-11-19* (`tddy-remote-shell`), small PR: when the marker came on stdout
   (pty-merged, `coder.rs:328-334`) and the status is non-zero, carry the merged command output as
   `stderr` (the streams cannot be told apart, and the CLI's wrapper text is noise once the marker
   proves the command ran). An exact alternative is to have the one-shot wrapper send the command's
   stderr to a file on the workspace and frame it back after a second marker. `stderr_tail`,
   `success_or_exited` and `target_words` then all improve with no change of their own.
3. ~~**P1. A remote cut bases on `<remote>/<main>`**~~ — *landed 2026-11-19 (resolved on the target's clone after its fetch, option (b) — (a) broke a project with no URL, whose target clone has no `origin`)* (`tddy-agent-launch` and `tddy-git`), small PR:
   resolve the default as the local cut does and pass it as `base_ref` instead of `"HEAD"`
   (`cli_session_worktree.rs:292-297`). Pin it with the `remote_cut_tests` module in the same file.
4. **P2. Usage context of a target session** (`tddy-agent-launch`, `tddy-usage`): record the
   target's home (from the existing home probe) and the target. Until usage is read over the shell,
   the usage inspector should say "on a target" rather than read the operator's local
   `~/.claude/projects`. This can be its own TODO if deferred.
5. **P2. Log a failed start** on the daemon log, with its error (§3, last bullet), so that the next
   opaque failure has a line to grep.

**What would close this file:** fixes 1 to 3 merged, and a Claude start on `mantasi/remote-dev`
reaching its prompt from a fresh worktree on `origin/master`, with `SessionStart` reaching the
session's status through `hooks.log`. The last check also narrows the real-deployment TODO above.

## Leftover state (not modified)

- Workspace: `~/.tddy/repos/qape-hq/.worktrees/01a1f24f-7302-7521-b951-e380ae501dd5` (branch
  `test-coder-remote-dev` at `9763fd02c7`, `.claude/`, `.tddy/{hooks.log,session.json}`), and
  `…/.worktrees/01a1f24c-c50c-7f62-87a3-0905764ddbce` (branch `probe/coder-remote-dev-worktree`, from
  the identical failed start `start-1795066479874-49443-1`).
- Host: `~/.tddy/sessions/01a1f24f-…` (`agent-system-prompt.md`, `answer-wait-ms`, `changeset.yaml`
  in state `Init`, `usage-context.json`).
