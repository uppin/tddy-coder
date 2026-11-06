# The hand-built `claude-cli` launch outlives the `claude` spec

**Detected:** 2026-11-02, `/pr-wrap` on [PR #791](https://github.com/uppin/tddy-coder/pull/791)
(`#agent-tools 2/12`, claude-host).

The unsandboxed `claude-cli` start and resume launch from the `claude` tool definition whenever the
session host carries `HostCodingAgents`, and `tddy-daemon` always installs it. The hand-built
path (`CliSessionManager::start_with_options` / `resume_with_options`, `install_claude_cli_hooks`
writing the hooks file, `MainAgentFlags::hand_built_args`) is still taken when a host has no coding
agents. That covers the session-lifecycle suites that build a `DaemonSessionHost` without
`with_coding_agents`, and any embedder that does the same. Until it goes, two renderings of the same
launch must be kept byte-identical by hand.

**Owner:** `#agent-tools 12/12` agent-registry ([PR #801](https://github.com/uppin/tddy-coder/pull/801)).
It removes daemon.yaml `claude_cli:` and moves the session-type gates onto spec capabilities, which
is the point where nothing needs the hand-built argv. #801's changeset does not yet list this
removal.

**What would close it:**
- `coding_agents` is no longer optional on `LaunchSessions` / `DaemonSessionHost`.
- The hand-built start, resume and hooks branches are deleted.
- The suites that relied on them install a fixture `HostCodingAgents`.

Marker: `TODO(agent-tools 12/12)` on `HostCodingAgents` in
`packages/tddy-agent-launch/src/spec_launch.rs`.
