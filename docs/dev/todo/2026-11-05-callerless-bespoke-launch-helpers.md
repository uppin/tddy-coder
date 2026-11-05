# 2026-11-05 — Helpers left callerless by the claude and cursor launch collapse

**Category:** Gap (unplanned deletions, left for approval)
**Owner:** `#agent-tools` n17 `cli-dispatch-collapse` ([#823](https://github.com/uppin/tddy-coder/pull/823)); the sweep node (n20) deletes them once approved.
**Source:** `#agent-tools` 17/19 (green)

Deleting the planned bespoke claude and cursor launches (§ Removed of
`docs/dev/1-WIP/2026-11-04-agent-tools-cli-dispatch-collapse.md`) left these items with no production
caller. None is in that list, so none was deleted: each carries `#[allow(dead_code)]` and a
`TODO(cli-dispatch-collapse)` naming this record.

| Item | Where | Why it survives |
|---|---|---|
| `warm_up_jail_agents`, `jail_semantic_index_env` | `tddy-agent-launch/src/jail_launch_steps.rs` | the bespoke jailed claude start was their only caller; the generic jailed start warms and indexes through `agent_roster` and `cli_session_worktree::semantic_index_env` |
| `project_default_branch_ref`, `create_jail_project_worktree`, `link_jail_branch_to_stack_node` (and `JailBranch`) | `tddy-agent-launch/src/jail_worktree.rs`, `svc_start_sandboxed_claude_cli_session.rs` | the generic jailed start cuts its worktree with `provision_cli_worktree` |
| `create_routed_session_dir` | `tddy-agent-launch/src/claude_cli_spawn.rs` | only `session_provider_route_tests` call it |
| `chat::{cursor_notify_env, resumed_cursor_notify_env, cursor_hook_daemon_url}`, `cursor_from_spec::NodeLauncher` users | `tddy-agent-launch/src/chat.rs` | public; the generic launch notifies through `notify_env` |
| `relaunch_jail_steps::relaunch_jail_env` and the `Relaunch*` records in `svc_relaunch_sandboxed_runner.rs` | `tddy-agent-launch` | already `TODO(harness-clis)` orphans; the bespoke relaunch was the last user of the module's types |

Deletion needs the developer's approval (AGENTS.md § Judgment Boundaries).
