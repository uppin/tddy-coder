# 2026-11-18 — A target launch runs the daemon host's resolved agent path and environment on the target

**Category:** Known defect (found by reading the code; no target start has reached it yet)
**Source:** the Coder `remote-dev` smoke test of 2026-11-18 (orchestrator session `01a1ecee-e942-7982-abfc-104c6c4baa98`, diagnostics session `01a1ecfc-0fac-77a0-bb0d-ac20a90593ae`), left open by the fix that made Coder one-shots cross the pty intact ([`2026-11-18-coder-one-shots-cross-the-pty-intact`](../changesets/2026-11-18-coder-one-shots-cross-the-pty-intact.md))

`HostCodingAgents::resolve` (`tddy-agent-launch/src/spec_launch.rs`) resolves the agent's
`{{binary}}` on the daemon's host, and `launch_on_target`
(`tddy-agent-launch/src/remote_agent_launch.rs`) renders the argv from it: a Claude start on a Coder
workspace would `exec env -i … '/Users/<user>/.local/share/claude/versions/<v>' …` there. That path
exists only on the daemon's host. The `env -i` environment is likewise built from the daemon's
(`AgentEnvBase::agent_env` over `std::env::vars_os()`), so `PATH` and `HOME` would be the
daemon host's. The tests launch on the same machine, where both paths exist, so they cannot see it.
The Claude start never got this far on `remote-dev`: it hung writing its settings file first, which is
fixed.

**Confirmed live 2026-11-18** (`mantasi/remote-dev`, after #941 opened the terminal): as the daemon
resolves the agent, the launch ends with `env: '/Users/mantasi/.local/share/claude/versions/2.1.311':
No such file or directory`, exit 127. With the target's binary, `HOME` and `PATH` given by hand
(`tddy-session-driver --binary … --env …`), the daemon host's **`TMPDIR`** still reached the agent:
Claude died with `EACCES: permission denied, mkdir '/var/folders'`, and started only with
`TMPDIR=/tmp`. The leak is the whole environment the daemon host's base set keeps, not only `PATH`
and `HOME`.

**Why deferred:** which binary a target runs is a design decision: the spec's bare command resolved by
the target's login shell, a per-target configured path, or a probe at start. It is shared with
[2026-11-18-cursor-chat-is-minted-on-the-daemon-host-for-a-target.md](2026-11-18-cursor-chat-is-minted-on-the-daemon-host-for-a-target.md).

**What would close it:** resolve the agent on the target, and take its `HOME` and `PATH` from the
target. Pin both with a stand-in target whose home and binary path differ from the test host's.
