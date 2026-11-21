# 2026-11-21 — A target agent that dies at start fails the start on a slow transport

An agent started on a target (a Coder workspace or an SSH host) that exits right after starting now
fails the start with its last output even when the target's round trip is longer than the startup
window; each liveness check has the start's allowance to answer. A target that answers no check
within the window plus that allowance fails the start with an error naming the bound, and leaves no
session. See [Claude CLI session § Agent on a target](../claude-cli-session.md).
