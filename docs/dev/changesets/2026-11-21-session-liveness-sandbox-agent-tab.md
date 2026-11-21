# 2026-11-21 — A sandboxed session's agent tab runs in a jail of its own

**Type:** Feature · **Stack:** `#session-liveness` 7/8 (PR #966) · **Packages:** `tddy-terminal-rpc`,
`tddy-daemon-sandbox`, `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-coder`, `tddy-session-web`,
`tddy-client-web` (generated), `tddy-testing-commons` · Feature:
[session-liveness.md](../ft/daemon/session-liveness.md#a-sandboxed-sessions-tabs)

## Problem

After #964 only a *shell* could run in a tab jail: `sandbox` around an agent was refused, so an extra agent
tab of a sandboxed session either ran unjailed on the host or could not be opened, was never counted as the
session's live agent, and was dropped on Resume.

## What changed

- **`tddy_daemon_sandbox::tab_jails`** — `TabJailAgent { harness, conversation_id, resume }`;
  `TabJailSpawn::spawn_agent` beside `spawn_shell`; `TabJails::start_agent` (registered and watched exactly as
  `start`); `TabJails::live_agent_sessions` (the sessions with a jailed **agent** tab whose runner runs — a
  jailed shell never counts) and `agent_harness`. The registry remembers which jails are agents in a private
  `RegisteredJail`; its public surface is otherwise unchanged.
- **`tddy_agent_launch::tab_jail_agent`** — `TabJailAgentPlanner::plan` / `plan_on_route`: the agent's grants
  lowered from its tool closure as the main's jail is (`lower_tool_jail`, on the session's provider route), its
  argv rendered from the spec's `run:` template in the **tab's** conversation (`agent_tab_argv`), and the exec
  plan written under the tab's own `TabJailPaths`. It writes **none** of the session's records — the role
  `launch_jailed` records before its spawn and the usage context it records once the home is known are the
  main's. `impl TabJailSpawn for LaunchSessions::spawn_agent` spawns, waits for and bridges the runner as the
  shell's does. A persistent agent home the main's jail also uses is used as is, with no second credential
  bootstrap. `LaunchSessions::live_sessions` unions in `tab_jails.live_agent_sessions()`.
- **Shared with the main's launch, not grown:** `needs_egress_shim` / `pick_egress_shim_port` are extracted from
  `launch_jailed` (it shrinks by 5 lines); the tab-jail launch writes the exec plan before `run_tab_jail`, which
  takes the agent's `route` (a shell passes none).
- **Terminal wire** — `sandbox` around a CLI agent kind is `TerminalKind::SandboxedAgent { agent }`
  (`from_wrapped_request` routes it ahead of the shell case); around `tmux`, nested in `sandbox`, or on an
  unsandboxed session it stays refused. The participant's `CoderTerminalRoster` refuses it
  (`FAILED_PRECONDITION`). The `wrappers` proto comment (and generated TS) says so.
- **Roster** — `start_jailed_agent_tab` mints the tab's conversation by `new_agent_tab`'s rule and records
  `[sandbox, agent]` with the harness, conversation and `resumable`; a jail or a record that fails stops the
  jail. A jailed agent tab is listed as its agent's kind. `restore_plan` plans it as
  `TabRestore::JailedAgent`: a new jail, **resuming** the recorded conversation where the harness can (claude,
  cursor) and **fresh, reported `FRESH` with why** where it cannot (opencode, muse, warp). `RosterRestore`
  restores it through `restore_jailed_agent`.
- **Web** — `NewTerminalMenu` lists every agent kind under "In sandbox" for a sandboxed session, each opening
  with `wrappers: [sandbox]`.

## Wrap cleanups

- `restore_tabs/roster.rs`'s `carry_out` (61 lines at the merge base, 65 at the green) is 58 again: the arms
  that only succeed with `Restored::AsRecorded` go through a `recorded` helper.
- The sleep-runner `TabJailSpawn` stand-in and `a_launch` were copied between `tab_jails_tests.rs` and
  `tab_jails_agent_tests.rs`; both now use `tab_jails_test_support.rs`.
- No file this PR touched is at or over 500 production lines (largest: `roster.rs` 494,
  `tab_jail_launch.rs` 451, `jail_launch.rs` 450).
- **Flake fixed:** `GhCliStub::build` (`tddy-testing-commons`) now writes the fake `gh` through
  `write_executable`, which warms it up — the first exec of a fresh script costs seconds on macOS and ate
  the probe deadline of the tests that spawn it.

## Verification

Scoped to the touched packages: clippy `-D warnings` over `tddy-agent-launch`, `tddy-daemon-sandbox`,
`tddy-terminal-rpc`, `tddy-session-lifecycle` and `tddy-testing-commons` (all targets); `tddy-agent-launch --lib`
(340), `tddy-daemon-sandbox` `tab_jails` (19), `tddy-session-lifecycle --lib` (189) and its
`sandbox_agent_tab` (5), `sandbox_shell_tab` (11) and `resume_tab_roster` (16) acceptance suites,
`tddy-terminal-rpc`'s sandbox kind tests, and the Cypress `NewTerminalMenuSandboxAcceptance` spec (3).
Whole-workspace health is CI's.

## Known gaps

- **The kernel sandbox is still not tested by CI** — every behaviour above is pinned against an in-memory
  `TabJailSpawn` over real processes ([`2026-11-02-ci-runs-no-linux-jail`](../todo/2026-11-02-ci-runs-no-linux-jail.md)).
- A jailed claude tab shares the session's hooks and loads the project's `.mcp.json` as the main's jail does
  ([`2026-10-24-claude-agent-tab-shares-session-hooks`](../todo/2026-10-24-claude-agent-tab-shares-session-hooks.md),
  [`2026-11-02-jailed-claude-loads-project-mcp-json`](../todo/2026-11-02-jailed-claude-loads-project-mcp-json.md)).
- Agent tabs on a target are `target-agent-tab` (8/8).
