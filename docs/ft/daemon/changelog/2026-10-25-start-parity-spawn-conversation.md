# 2026-10-25 — A spawning session starts any session through the same start as StartSession

`spawn_conversation` is no longer a narrower start path (grill-me only, `claude-cli` only, this host
only, no progress, no cancel). It takes `StartSession`'s arguments and runs the very start a
project-home session's `StartSession` runs: any session type, on any eligible host, with every
New-session option, recorded in the orchestrator's start ledger and answered at once with a handle.
The agent follows the start with `spawn_conversation_status` and stops it with
`spawn_conversation_cancel`. A session that can spawn conversations is also offered the four start
tools — `StartSession`, `GetSessionStart`, `CancelSessionStart`, `DescribeSessionStartOptions` — for
its own project. A `claude-cli` conversation now names its own model. See
[Conversation spawners](../project-concept.md#conversation-spawners).
