# 2026-11-21 — A target session starts without the agent on the daemon host

A [session started or resumed on a target](../claude-cli-session.md#the-generic-cli-spec-launch-serves-claudes-needs)
runs the target's own agent: its bare command, found on the target login's `PATH`, so a daemon host with no copy of the
agent drives a target that has one. Before the worktree is cut, the target's login shell is asked for the command: a
target without it fails the start or resume `FAILED_PRECONDITION` naming it (an unreachable one `UNAVAILABLE`), with
nothing cloned, cut or recorded.
