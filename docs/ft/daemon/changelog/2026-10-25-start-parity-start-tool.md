# 2026-10-25 — An orchestrator starts any session a person can, on any host

The project-home session's start tool is `StartSession`. It takes every option the New-session form
offers — session type, host, agent, recipe, model, provider account, branch intent and names,
permission mode, stack parent, sandbox and codebase placement, semantic index, specialized agents and
host-document attachments — and its schema lists the closed choices, including the eligible hosts and
supported recipes. A start naming another host creates the session there, and that host records it
as started by the home session. A branch another session owns is refused, naming the branch. An
orchestrated `tool` session start is refused for now. See
[StartSession](../project-concept.md#startsession).
