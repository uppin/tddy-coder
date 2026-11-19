# 2026-11-18 — Claude and Cursor start on a target with its own binary, env, chat and trust

**Type:** Fix · `#coder-start` 7/10, PR [#947](https://github.com/uppin/tddy-coder/pull/947)

`target_login_exec` / `target_agent_one_shot` run the target's own agent under its login shell; `target_trust` pre-trusts a Claude clone root in `~/.claude.json`; `RemoteLaunch.clone_root`, `clone_root_of_worktree`; `plan_the_launch` binds `on_target`; the Cursor mint runs through the target shell. Full record:
[`docs/dev/changesets/2026-11-18-target-agent-start.md`](../../../../docs/dev/changesets/2026-11-18-target-agent-start.md).
