# 2026-11-18 — Claude and Cursor start on a target with its own binary, env, chat and trust

**Type:** Fix · `#coder-start` 7/10, PR [#947](https://github.com/uppin/tddy-coder/pull/947) · **Packages:** `tddy-agent-launch`, `tddy-agent-backend`, `tddy-session-driver`, `tddy-coding-agent-specs`

## Problem (live on `mantasi/remote-dev`)

A target start rendered the daemon host's binary path (exit 127) and the daemon host's
`HOME`/`PATH`/`TMPDIR` (`EACCES` on `/var/folders`); Cursor minted its chat on the daemon host
with the target's worktree as `current_dir`; Claude stopped at the folder-trust and project-MCP
dialogs on a pty nobody watches.

## What changed

- **The target's own agent and environment.** `remote_agent_launch::target_login_exec` runs
  `exec "${SHELL:-/bin/sh}" -lc 'exec env -i HOME="$HOME" PATH="$PATH" TMPDIR="${TMPDIR:-/tmp}" <spec env> <bare command> <args>'`;
  the base set is expanded on the target, only the spec's explicit env (`run.env`, session env,
  `pass_env` values) travels. `plan_the_launch` renders with `tool.binary = tool.command` and no
  node launcher.
- **Cursor's chat minted on the target.** `ConversationStart.target: Option<ConversationTarget { shell, command }>`;
  the backend's port `AgentOnTarget::run(args, deadline)` and `ConversationIdAdapter::mint_on_target`
  (no `tddy-remote-shell` dependency in `tddy-agent-backend`); `tddy-agent-launch` implements it with
  `target_agent_one_shot`. The id is the last stdout line (login banners). A local mint into a missing
  directory now names the directory.
- **Claude pre-trust.** `target_trust::pre_trust_clone_root` (new): `projects["<clone root>"].hasTrustDialogAccepted`
  and every `.mcp.json` server in `disabledMcpjsonServers`, written `OwnerOnly` only on change; a failure is
  `RemoteLaunchError::Trust`. `RemoteLaunch.clone_root` comes from `clone_root_of_worktree` (`<root>/.worktrees/<session id>`).
  Only for a spec whose hooks adapter is Claude's.
- **Cursor trust on a target** (found by the live check, added at [W]): `plan_the_launch` binds the
  `on_target` flag and `cursor.yaml` renders `--trust` for it. Local starts are unchanged (the sandboxed
  Cursor acceptance test still pins no implicit `--trust`).
- `tddy-session-driver` passes the clone root; `--binary`/`--env` are overrides now, not workarounds.
- Flaky `spec_headless_turn_deadline` test: a fresh stub's first-exec scan ate the 3 s deadline; the
  stub is warmed up first.

## Decisions

- Bare command through the target's login shell; pre-trust over answering dialogs; MCP choice reject-all (developer, 2026-11-18).
- Mint through a port, not a backend → remote-shell dependency; clone root derived, not plumbed (orchestrator, under the user's finish-without-stopping instruction).
- **MCP key location — kept in `~/.claude.json`** (orchestrator, under the user's finish-without-stopping instruction).
  Live on Claude 2.1.327: a start with `disabledMcpjsonServers` in `~/.claude.json` reached its prompt, and
  Claude itself moved the key into the project's `.claude/settings.local.json` and dropped it from the
  `~/.claude.json` record — the legacy key is honoured through that migration, so no production change.
  Caveat: a control start with no rejection also showed no MCP dialog on this workspace, so the live run
  proves where the key lands, not that it suppresses a dialog.
- **Cursor workspace trust — fixed here** (orchestrator, under the user's finish-without-stopping instruction):
  a never-seen target path opened on "Workspace Trust Required"; with `--trust` it reached the prompt.
  Rendered only on a target, so the local boundary holds.

## Live evidence (`mantasi/remote-dev`, 2026-11-18)

Through the exact `remote_agent_command` form: Claude 2.1.327 reached `❯` with `.tddy/agent.pid` naming
the `claude` process; `cursor-agent create-chat` minted a chat id through `target_agent_one_shot`'s form,
and `cursor-agent --trust --resume <id>` reached "→ Plan, search, build anything" in a fresh path. Test
state on the workspace was removed afterwards.

## Verification

- [G]: 604 pass / 0 fail `tddy-agent-backend` + `tddy-agent-launch`; `tddy-session-driver` 10 / 0; clippy clean.
- [W], after the Cursor trust change: `./test -p tddy-coding-agent-specs -p tddy-agent-launch` 604 pass / 0 fail;
  `./dev cargo clippy -p tddy-agent-launch -p tddy-agent-backend -p tddy-session-driver -p tddy-coding-agent-specs --all-targets -- -D warnings` clean.
- Code issue `complexity-remote-agent-launch-launch-on-target`: 59 → 60 lines (budget 60), 6 parameters unchanged.
