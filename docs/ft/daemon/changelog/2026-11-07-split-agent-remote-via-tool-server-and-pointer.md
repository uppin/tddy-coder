# 2026-11-07 — A split session's agent carries no `TDDY_*` and reads the host's agent spec

A split session's agent process is given no `TDDY_*` variable: the transport to its codebase session
is registered on its `tddy-tools` server and written to a `.tddy/session.json` in its context dir,
removed when the agent exits. Its argv, hooks and guidance come from the daemon's coding-agent
registry, so an operator's override of the agent applies to split sessions too.

See [the change](../../../dev/changesets/2026-11-07-split-remote-env-via-pointer.md).
