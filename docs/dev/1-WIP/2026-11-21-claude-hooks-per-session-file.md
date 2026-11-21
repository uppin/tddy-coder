# A claude-cli session's hooks are a file of its own, rendered on every launch

**Type:** Fix
**Packages:** `tddy-agent-backend`, `tddy-coding-agent-specs`, `tddy-agent-launch`, `tddy-session-split`
(tests: `tddy-session-lifecycle`, `tddy-session-driver`, `tddy-telegram-control`)

## Problem

Reported 2026-11-21: after `/clear` in the project-home session (`01a16dc6-…`, claude-cli on the
home worktree), the new conversation's tool calls never reached the session's activity, and the
next resume reopened the conversation from before the clear.

Neither was a `/clear` defect. The rotation (`#orchestrators` 6/11) records a new conversation from
the `SessionEnd{reason: clear}` / `SessionStart{source: clear}` hook pair, and a resume binds the
newest recorded one. No hook of the home session reached the daemon at all:

- Claude's hooks were one worktree-wide file, `<worktree>/.claude/settings.local.json`, each
  command naming a session id, its hook token and the daemon's agent-tool socket.
- A resume rendered hooks again only for an argv adapter (`resumed_argv_hooks`); a file adapter's
  file was "kept as its start wrote it".
- The project-home worktree also hosts the project's orchestrator sessions. One started there on
  2026-11-18 overwrote the file with **its** session id and token.
- The agent-tool socket's name carries the daemon's pid (`tddy-agent-tools-<pid>-<n>.sock`), so
  after any daemon restart the kept file named a socket nothing listened on.
- `tddy-tools session-hook` is fail-quiet, so nothing reported either.

A daemon restart alone is enough to break hooks for any resumed claude-cli session, home or not.

## Change

- **Per-session file.** The `claude-settings-local` adapter renders
  `<workdir>/.tddy/hooks/<session_id>/claude-settings.json`
  (`tddy_agent_backend::claude_hooks::session_file::claude_session_hooks_file`), and
  `slots::render` adds the adapter's `file_load_args` — `--settings <file>` — for every file it
  renders. The routed-launch-only `--settings` in `render_host_launch_slots` is gone: the route's
  pin is in the same file and is always loaded.
- **Every resume renders the hooks.** `resumed_argv_hooks` → `resumed_hooks`: any adapter's hooks,
  rebuilt around the recorded token, with the resumed route's plain env pinned as a start pins it.
- **Worktree preparation** (`tddy-agent-launch::session_hooks_worktree`), on start and resume,
  locally or through the target's remote shell, before slot files are written:
  - `/.tddy/hooks/` is added to the checkout's `info/exclude`;
  - the daemon's entries are released from the legacy `.claude/settings.local.json`
    (`HooksAdapter::legacy_shared_file` / `release_legacy_shared_file`): its `session-hook` and
    hook-log commands, and the route `env` pin the daemon wrote beside them. Anything else (Claude
    Code's saved `permissions`, the operator's own hooks) is kept; a file left empty is removed.
    Claude Code merges hooks across settings sources, so left there they would fire beside the
    session's own file.
- **Split sessions** keep their hooks at `context/.claude/settings.local.json` in their private
  context dir (`split_hooks_file_in`), loaded by the `host_agent` variant's `hooks_flag` as before.

## Behaviour across `/clear` (unchanged, now reachable)

The session's conversation is the newest entry of `.session.yaml` `agent_conversations`. A
`/clear` appends the `SessionStart{source: clear}` session id as a `rotate` entry; activity,
transcript and usage follow it, and a resume runs `--resume <newest>`.

## Not done

- A session that was already broken keeps the conversation it last recorded: the home session's
  record still ends at `8c7017b2`, not the `1e774bed` conversation the unrecorded clear began.
- An agent kept alive in tmux across a daemon restart (`#session-liveness` 2/8) keeps the hooks it
  loaded at launch, naming the old socket. A stable socket path would close it; not decided.
- The jailed launch (`write_jail_hooks`) is untouched: a jailed Claude session runs hookless.

## Docs to update at wrap

`docs/ft/daemon/claude-cli-session.md`, `docs/ft/daemon/terminal-sessions.md`,
`docs/ft/daemon/remote-managed-worktree.md`, `packages/tddy-coding-agent-specs/docs/coding-agent-specs.md`,
`packages/tddy-agent-launch/docs/module-layout.md`, `packages/tddy-session-driver/docs/architecture.md`
— each still names `.claude/settings.local.json` as where a session's hooks live.
