# 2026-11-20 — An agent on a target has a configurable minute to start

An agent started on a target now has `remote_agent_start_timeout_secs` (daemon YAML, default 60)
to record its pid there, instead of a bound derived from the startup window (`× 10`, at least
5 s). A Coder workspace's terminal alone takes ~10 s to run its command, so every claude-cli start
on a Coder target failed with "did not record its pid within 5s". `tddy-session-driver` takes the
same bound as `--start-timeout-secs` (default 60). See [claude-cli-session.md](../claude-cli-session.md).
