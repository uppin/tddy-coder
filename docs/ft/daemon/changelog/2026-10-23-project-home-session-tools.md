# 2026-10-23 — Project-home tools: list, start, read and diff a project's sessions

A project-home session's agent can list its project's other sessions with their status, start a
session of any type in the project (recorded as started by the home session), read a file or list a
directory in another session's worktree, and diff that worktree against its base. Only a
project-home session may run them, and only on its own project. The agent is offered exactly the
tools its role allows — `ListExecTools` answers per session — and the home agent is started with
them pre-approved. The role is recorded before the agent starts, and a client cannot request it:
`StartSession` refuses a `session_role`. See
[project concept § Project-home tools](../project-concept.md#project-home-tools).
