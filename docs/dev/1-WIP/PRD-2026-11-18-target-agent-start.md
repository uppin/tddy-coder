# PRD: Claude and Cursor start on a target with the target's own binary, environment, chat and trust

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Stack**: `#coder-start` 7/10 — branch `feature/coder-start/target-agent-start`, base `feature/coder-start/idle-status`

## Problem

After #939–#941 a session's terminal opens on a Coder workspace, but neither agent gets up. Each
failure below was confirmed live on `mantasi/remote-dev` on 2026-11-18:

1. **The daemon host's binary and environment are sent to the target.** The launch renders
   `exec env -i … '/Users/<user>/.local/share/claude/versions/<v>'`, which is a path that exists only on the
   daemon's Mac. The result is exit 127. With the target's binary given by hand, the daemon host's
   `TMPDIR` (`/var/folders/…`) still reached Claude, which died with `EACCES`. The whole base set
   comes from `std::env::vars_os()` on the daemon.
2. **Cursor mints its chat on the daemon host.** `cursor-agent create-chat` is spawned locally with
   the target's worktree path as its `current_dir`. The `chdir` fails, and the error blames the
   binary. Even if the directory existed, the chat would land in this host's Cursor store.
3. **Claude stops at two dialogs.** The first is "Is this a project you created or one you trust?",
   which defaults to *exit*. The second is "N new MCP servers found in this project". No one watches
   that pty. Claude keeps both answers per repository, so a later worktree of the same clone shows
   neither.

## Requirements

1. **The agent on a target is found by the target.** The spec's bare command (`claude`, `agent`) is
   run through the target's login shell. `HOME`, `PATH`, `TMPDIR` and the rest of the base set come
   from the target. The launch adds only the spec's explicit env (`run.env`, the session env, and
   `pass_env` values), never a daemon-host value. A daemon `binary_path` override applies to local
   starts only.
2. **Cursor's chat is minted on the target.** It runs `create-chat` through the target shell in the
   target's worktree, with the same binary and env rules. A local spawn whose working directory is a
   target path is refused, and its error names the directory.
3. **The target's clone is pre-trusted.** Before a Claude start on a target, the target's
   `~/.claude.json` records the clone root (`~/.tddy/repos/<repo>`) as trusted, with the repository's
   project MCP servers (`.mcp.json`) already answered **reject all**. Every worktree under it starts without a dialog. Other entries in the file
   are kept.
4. Local starts are unchanged.

## Acceptance criteria

- [ ] On a stand-in target whose `HOME`, `PATH`, `TMPDIR` and agent location all differ from the test
      host's, a launch runs the target's agent, and the agent sees the target's `HOME`/`PATH`/`TMPDIR`
      plus the spec's env and nothing else from the daemon host.
- [ ] A Cursor-style minted start on a target mints through the target shell in the target's
      worktree. A local mint into a missing directory fails naming that directory.
- [ ] After a Claude start on a target, the target's `~/.claude.json` trusts the clone root, records
      every `.mcp.json` server as rejected, and keeps its other keys. A second start leaves it unchanged.
- [ ] Live: a Claude start and a Cursor start on `mantasi/remote-dev` each reach their prompt
      without manual help. The orchestrator records this in the PR.
- [ ] Scoped gates: `./test -p tddy-agent-launch -p tddy-agent-backend -p tddy-session-driver`, and
      clippy with `-D warnings`.

## Out of scope

- Answering a PermissionRequest or AskUserQuestion on a target. The hook follower is one-way.
- Per-target agent configuration.
