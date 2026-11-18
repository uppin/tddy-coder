# 2026-11-18 — An agent's terminal on a Coder workspace opens, and its start is judged at the deployment's pace

**Type:** Fix · `#coder-start` 2/3, PR [#941](https://github.com/uppin/tddy-coder/pull/941)
**Packages:** `tddy-remote-shell`, `tddy-agent-launch`

A live run of `launch_on_target` against `mantasi/remote-dev` (coder v2.33.5) failed three times
before any agent could start. Each cause was invisible to the stand-in suites, which are handed the
client by absolute path, accept any flag and answer instantly.

- **The client was not found.** The session's local PTY runs on a cleared environment, so a bare
  `coder` (or `ssh`) failed with `Unable to resolve the PATH`. The PTY now also carries the daemon's
  own `PATH`, the client's lookup path. The agent's environment is unchanged and still travels
  inside the remote command.
- **The CLI refused `-t`.** `CoderShell::interactive` is now `coder ssh --disable-autostart
  <workspace> -- <command>`. v2.33.5 has no `-t` and gives the command a terminal when its stdin is
  one, which is the caller's PTY.
- **The start probes were shorter than one call.** The pid-file probe and the liveness probe were
  bounded by a fixed 5 s while one round trip took 6-8 s, so the launch failed `AgentNeverStarted`
  after 150 s with Claude running on the workspace. Each probe is now bounded by the time left in
  the start bound, or in the startup window, so an agent that exits during the window still fails
  the launch.

**Pinned by** tests that fail before the fix: `a_coder_found_only_on_the_daemons_path_carries_the_agents_terminal`
(its own binary, `coder_agent_launch_on_path_acceptance`), `the_interactive_command_opens_a_terminal_on_the_workspace`,
a stand-in CLI that refuses `-t`, and `an_agent_on_a_deployment_slower_than_five_seconds_a_call_is_launched`
with `an_agent_that_exits_on_a_slow_deployment_fails_its_launch_with_its_last_words` as its guard.

**Run against the real deployment:** with the VM's binary, `HOME` and `PATH` put in by the test, the
launch is judged started and Claude completes its turn. As the daemon resolves the agent, the launch
still stops at the daemon host's agent path (exit 127); that stays in the backlog.

**Gates (scoped):** `./test -p tddy-remote-shell -p tddy-agent-launch` exits 0, and `cargo clippy
--all-targets -D warnings` is clean on both packages.

**Backlog.** Open and not this change: `2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path`,
`2026-11-18-claude-on-a-target-will-meet-the-folder-trust-prompt`, and
`2026-11-16-remote-shell-test-fixtures-are-duplicated` (this change added one more stand-in
behaviour to its duplicated fixtures).
