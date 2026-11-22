# 2026-11-19 — A start on a target still requires the agent's binary on the daemon's host

**Category:** Design question (product decision, not a defect)
**Status:** Open — partially fixed: the **new-tab half** is resolved by #967 (`#session-liveness` 8/8); the
**session-start half** below stays open.
**Source:** the `#coder-start` stack's `[F]` stage on #948 (commit `4df6979e2`), raised to the user by
the `#coder-start` sweep (10/10). No code was changed for it.

Since #947 (`#coder-start` 7/10) a start on a target runs **the target's own** agent: `plan_the_launch`
(`packages/tddy-agent-launch/src/remote_agent_launch.rs`) replaces the binary this host resolved with
the spec's bare `binary.command` (`claude`, `cursor-agent`), found on the target's login `PATH`. But
the agent is still **resolved on the daemon's host first**, before anything knows the start is
addressed at a target:

- `resolve_session_agent` (`packages/tddy-agent-launch/src/harness_from_spec.rs`) calls
  `HostCodingAgents::resolve(kind.spec_id, None)`, which runs the spec's binary resolution against
  this host's `ToolHost` (`binary.env`, `$HOME/.local/bin`, `PATH`) and refuses with
  `tool <id> is unavailable: searched …` when none holds it.
- The resolved `HostAgent` is what the start carries throughout (`CliSessionStart.agent`), and its
  host path is still read for host-only steps (`node_launcher_of(&agent.tool.binary)` on a host start,
  the conversation's host mint on a host start).

So a daemon host **without** `claude` installed cannot start a Claude session on a Coder workspace or
ssh target that **has** it — the start is refused with "tool claude is unavailable" before any remote
call. #948's `[F]` stage met exactly this on CI (no `claude` on the runner): two
`async_start_acceptance` tests passed only on a machine with `claude` installed, and were made
hermetic by resolving `claude` to a stub on the test host
(`a_host_as_current_user_with_operator_home_running_claude_as`), not by changing the behaviour.

## Resolved in #967: a new agent tab on a target

An agent tab opened "On target" no longer resolves the agent on the daemon's host: it runs the spec's bare command
found on the target's login `PATH`, mints its chat on the target and starts from a daemon host with no copy of the
agent (`target_agent_tab` / `new_target_agent_tab`; pinned by `a_target_agent_tab_starts_when_the_daemon_host_has_no_agent`).
Everything below now concerns **only a session start** on a target (`resolve_session_agent`,
`CliSessionStart.agent`); the options are unchanged, and option 2 now has a precedent in the tab.

`#tab-hardening` 1/4 (#989) adds the probe option 3 needs: `probe_agent_on_target`
(`remote_agent_launch/agent_probe.rs`) runs `command -v <agent>` through the target's login shell and fails
`AgentNotOnTarget` naming the command, and an agent tab now calls it. Option 3 for a start is therefore the
helper's second caller (`#tab-hardening` 4/4), not new machinery.

## Why it exists

The resolution predates targets: every start used to run on this host, so resolving the agent up
front was the correct "refuse before cutting a worktree" check. #947 changed what runs on a target but
deliberately left the resolution and its refusal unchanged (its scope was the target's binary, env,
chat and trust, not the host's preflight).

The spec's dependency closure is resolved the same way (`resolve_closure`), so a dependency
(`tddy-tools`, used by hooks and the MCP server) is also required on this host. For hooks that is
arguably right today, since the hook command a target runs is rendered from this host's path; for the
agent itself it is not.

## What deciding it needs

A product decision on what a start addressed at a target requires of the daemon's host. These are the options:

1. **Keep it** (today): the host must have the agent too. Simple, but it rules out a thin daemon host
   (a laptop with no agents installed driving Coder workspaces). Document it in
   `docs/ft/daemon/` as a requirement.
2. **Skip the host's binary resolution for a target start**: resolve the spec and its dependencies,
   but not the agent's own binary, when `target` is set. The target's absence of the agent then
   surfaces only at launch (`exit 127` inside the startup window, already reported with its
   command by #948), so the "refuse before a worktree is cut" property is lost for targets.
3. **Resolve on the target**: probe the bare command over the target's shell (`command -v claude`
   through `RemoteShell::exec`) before the worktree is cut. This keeps the early refusal, at the cost
   of one more remote call (about 6–8 s on Coder) per start.

Whatever is chosen also has to say what happens to the host-only uses of the resolved path on a target
start (today none run, see above) and whether `tddy-tools` keeps being required on the host.

**What would close it:** the decision recorded in `docs/ft/daemon/`, and (for 2 or 3) a test pinning a
target start from a host whose `PATH` has no agent, through `async_start_acceptance`'s stand-in target.
