# 2026-11-24 — A target session's hooks name their log by its absolute path

**Type:** Fix
**Packages:** `tddy-agent-backend`, `tddy-agent-launch`, `tddy-session-driver`

## Problem

On 2026-11-24 a Claude session on the Coder workspace `remote-dev` (session
`01a20c4d-8862-7201-81da-5f6d0ab37656`) refused every tool. The agent had `cd`-ed into
`model-hound/`, a subdirectory of its worktree, and from then on every hook failed:

```text
Stop hook: [printf '%s\t%s\t%s\n' 'Stop' '<token>' "$(cat | tr -d '\n')" >> '.tddy/hooks.log']:
/bin/sh: 1: cannot create .tddy/hooks.log: Directory nonexistent
PreToolUse:Bash hook error … Not run · permission denied
```

The agent ended up asking its user to `! mkdir -p model-hound/.tddy`.

Two facts produced this:

1. **The hook log was named relative to the agent's current directory.** A target start
   (`opencode_cli_spawn/start_steps.rs::log_hooks_on_the_target`) and resume
   (`opencode_cli_spawn/resume.rs::hooks_append_to_the_target_log`) set
   `HookCommandParams::remote_log` to `REMOTE_HOOK_LOG` (`.tddy/hooks.log`) on its own, and so did the
   session driver (`tddy-session-driver/src/hooks.rs`). Claude Code runs a hook in the agent's
   current directory, which follows the agent's `cd`. The launch created the log at
   `<worktree>/.tddy/hooks.log` and the follower tailed that absolute path, but the hooks looked for
   it wherever the agent had gone. One builder renders the command for Claude, Cursor and Muse, so
   all three were affected. The waiting hooks' answers directory (the log's directory joined with
   `answers`) and the hook-log report line a child on a target is told to run
   (`back_channel.rs`) were relative for the same reason.
2. **A failed append blocks the tool.** A Debian or Ubuntu target's `/bin/sh` is `dash`, which ends
   a failed `>>` redirection with status 2. Claude Code reads a hook's status 2 as a blocking error,
   so the failing `PreToolUse` hook refused the tool. macOS's `sh` ends the same failure with 1,
   which is why tests on a Mac never showed it.

The tests that cover target hooks (`remote_hooks_acceptance.rs`, `coder_agent_launch_acceptance.rs`)
built their `HookCommandParams` from `create_remote_hook_log`'s absolute path instead of going
through the launch's own wiring, and ran each hook from the worktree root, so neither fact was
exercised.

## Change

- `tddy_core::remote_hook_log_in(worktree)` is the session's hook log: the worktree's path joined
  with `REMOTE_HOOK_LOG`. `create_remote_hook_log` and `remove_remote_hook_log` use it.
- The start and the resume point a target session's hooks at it through
  `ClaudeCliHooks::point_at_the_target_log(worktree)`, with the worktree's path on the target. The
  answers directory, derived from the log's directory, is absolute with it.
- The session driver renders its hooks with the path `create_remote_hook_log` returned.
- `ChildPlacement::of` takes the child's target worktree (`Option<&Path>`, `None` for a local child)
  instead of a bool, and the hook-log channel names the absolute log. `ReportsTo::guidance_at` takes
  the same.

What is **not** changed, by the operator's decision: how a hook that cannot write ends. It stays a
loud failure — a blocking one under `dash` — so a log that cannot be written is never hidden; the
fix is that hooks always write. The waiting hook still waits out its answer wait after a failed
append; that is written up in
`docs/dev/todo/2026-11-24-target-waiting-hook-waits-after-a-failed-append.md`.

## Other ways the append could fail, checked

| Case | Finding | Pinned by |
|---|---|---|
| The agent's current directory is a subdirectory | Fixed: the log is named absolutely | `target_hook_log_tests::a_target_session_s_hooks_reach_its_log_after_the_agent_changed_directory`, `session_driver_hooks_acceptance::a_coder_session_s_hooks_reach_the_driver_after_the_agent_changed_directory`, `target_back_channel_shell_acceptance::a_report_run_after_the_child_changed_directory_reaches_its_worktree_s_log` |
| The worktree path holds spaces or quotes | Single-quoted for the shell; works from a subdirectory whose name holds a space | `target_hook_log_tests::a_worktree_path_with_spaces_and_quotes_still_takes_every_hook` |
| `dash` versus `bash` versus this host's `sh` | The command is POSIX; each appends from a subdirectory | `target_hook_log_tests::every_posix_shell_a_target_may_run_hooks_under_appends_from_a_subdirectory` |
| The waiting hook's answers directory | Absolute with the log | `target_hook_log_tests::a_target_session_s_waiting_hook_waits_for_its_answer_in_the_worktree` |
| `.tddy` missing when the first hook fires | Cannot happen at launch: `harness_launch::launch` runs `create_remote_hook_log` before `launch_on_target`, and its `write_file` runs `mkdir -p` on the log's directory; the start command also runs `mkdir -p .tddy/agents` before the agent's `exec` | `session_driver_hooks_acceptance` (`started` asserts the log exists once the agent is up) |
| A different user or directory inside tmux | The agent is started through the same shell, as the same user, under `env -i` with the target's own `HOME`; no `su`/`sudo`. With an absolute path the directory tmux starts in no longer matters | — |

Still possible, and not something a launch can guarantee: the agent itself deleting `.tddy` — for
instance `git clean -fdx`, which removes the files `info/exclude` lists. Its hooks then fail, loudly.

## Verification

- Scoped: `./test -p tddy-agent-backend -p tddy-agent-launch -p tddy-session-driver`;
  `./dev cargo clippy -p tddy-agent-backend -p tddy-agent-launch -p tddy-session-driver -- -D warnings`.
- Live on `remote-dev` (`/bin/sh` is `dash`), 2026-11-24, through `tddy-session-driver` with the real
  Claude Code: the agent ran `cd model-hound`, `pwd` and `cat README` as three Bash calls. The hooks
  after the `cd` carried `cwd: <worktree>/model-hound`, all nine (SessionStart, UserPromptSubmit,
  three PreToolUse, three PostToolUse, Stop) landed in `<worktree>/.tddy/hooks.log` and reached the
  driver, every tool ran, and no `model-hound/.tddy` was created. The installed `Stop` hook read
  `printf … >> '<worktree>/.tddy/hooks.log'`.
