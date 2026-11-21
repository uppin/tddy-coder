# 2026-11-21 — Children of an orchestrator report back to it

A session started by an orchestrator is told who its orchestrator is and to report its progress
and status periodically. It reports over exactly one channel: Claude Code's own peer messaging by
name between two Claude sessions on one host, `tddy-tools send` for any other local agent, or a
`TddyMessage` line in `.tddy/hooks.log` from a Coder or SSH target, which reaches the orchestrator as
an ordinary session message. Orchestrators and their Claude children are launched under stable
peer names. Agents without a system-prompt slot (Codex, Muse and others) are told ahead of their
first prompt. See [project concept § Reporting to the orchestrator](../project-concept.md#reporting-to-the-orchestrator).
