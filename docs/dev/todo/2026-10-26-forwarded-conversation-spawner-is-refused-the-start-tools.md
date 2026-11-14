# 2026-10-26 — A conversation spawner whose call is forwarded from another host is refused the start tools

**Category:** Deferred from `#start-parity` 9/9 (#758)
**Source:** `docs/dev/changesets/2026-10-25-start-parity-spawn-conversation.md`;
`packages/tddy-daemon-rpc/src/exec_tool/project_home_remote.rs` (`TODO(start-parity)`)

#758 offers a session that can spawn conversations (a `grill-me` session) the four start tools —
`StartSession`, `GetSessionStart`, `CancelSessionStart`, `DescribeSessionStartOptions` — for its own
project. Its **own** host serves them (`project_home_caller` in
`packages/tddy-daemon-rpc/src/exec_tool/project_home.rs`, by `session_tool_role`).

When the same call arrives on a host that does **not** hold the session (a split session's tools,
forwarded), that host confirms the caller by asking its peers' `ListSessions`, and accepts only a
listed `session_role` of `project-home`. A listed session carries no recipe, so the conversation-spawn
capability cannot be confirmed there, and the call is refused with "served only to a project-home
session".

**Why deferred:** confirming the capability remotely needs the listing (or a dedicated peer query) to
carry the session's recipe or tool role — a proto change outside #758's boundaries ("does not widen
which recipes get the spawn capability", no schema changes). `spawn_conversation` itself is not
affected: it runs on the spawner's own daemon over `TDDY_SOCKET`.

## What would close it

Carry the session's tool role (or recipe) in the peer's `ListSessions` row, and let
`confirmed_home_caller` accept `SessionToolRole::ConversationSpawner` for exactly the four start tools
(`is_conversation_spawner_tool`), mirroring `project_home_caller`. Test: a forwarded `StartSession`
from a grill-me session on host B is served on host A, and a forwarded non-start project-home tool is
still refused.

## Also: a stack node's read tools on another host (`#orchestrators` 7/11, 2026-11-14)

The same gap applies to the PR-stack tools served by the daemon
(`tddy_tool_engine::stack_tools`). A forwarded stack-tool call confirms its caller at the owner
through `ListSessions`, which reports the session's role but not its node binding, so only an
**orchestrator-role** caller is served across hosts. A session bound to a node of a stack held on
another host (`StackCallerRole::StackNode`) cannot use `pr_stack_status` / `pr_read` there: the
forwarded call is refused. The developer accepted the gap (2026-11-14). Closing it needs the same
`ListSessions` widening, or the owning host checking its own stack record for a node bound to the
caller — which it can do locally, since it holds the stack.

