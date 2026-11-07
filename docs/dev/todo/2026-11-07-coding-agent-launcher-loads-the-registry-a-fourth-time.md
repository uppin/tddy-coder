# 2026-11-07 — The coding-agent launcher loads the registry a fourth time

**Category:** Deferred from `#agent-tools` 22/28 (`feature/agent-tools/host-registry`, PR #830) — for the stack's sweep node
**Source:** `DaemonCodingAgentLauncher::for_daemon` in
`packages/tddy-session-lifecycle/src/coding_agent_launcher.rs` (`host_coding_agents(&tddy_data_dir)`)

## What is there

#830 made `runtime::build` load the daemon's coding-agent registry once, fail start-up when it does
not load, and hand the one `Arc<HostCodingAgents>` to Telegram, the reserved-ids step and the session
host. The headless/ACP launcher was outside that plan: `for_daemon` still calls
`host_coding_agents(&tddy_data_dir)` itself and, on failure, keeps the error in
`coding_agents_error` and refuses each headless conversation with it, rather than the start failing.
In practice the daemon's own load has already refused the same file, so this is a second read, not a
fallback — but it is a fourth load per daemon start and a second error path for one condition.

Three narrower leftovers of the same change, recorded so the sweep finds them together:

- `DaemonSessionHost::launch_coding_agents()` returns an owned `Arc` clone, not the `&Arc` the #830
  contract text named: a reference forced a clone at three call sites for no gain. Keep or change
  deliberately.
- `HarnessRouteSupport::new(Option<Arc<CodingAgentRegistry>>)`
  (`packages/tddy-daemon/src/provider_route_support.rs`) still takes an `Option`; its only production
  caller (`account_linking.rs`) now always passes `Some`. Only its own unit test passes `None`.
- `tddy_agent_launch::claude_cli_tmux::reattach_recorded_main` and the `harness_from_spec` dispatch
  family (`cli_launcher`, `recorded_cli_session_type`, `child_session_type`) still take
  `Option<&HostCodingAgents>`; `TmuxMainReattach` now always passes `Some`.

## What would close it

`DaemonLauncherDeps` takes the daemon's one `Arc<HostCodingAgents>` (`runtime::build` already holds
it in `BuildContext::coding_agents`) and `coding_agents_error` goes away; `HarnessRouteSupport::new`
and the agent-launch dispatch functions take the registry non-optionally, and their "no coding
agents" refusals read an empty registry (`no_coding_agents()`) instead of `None`.
