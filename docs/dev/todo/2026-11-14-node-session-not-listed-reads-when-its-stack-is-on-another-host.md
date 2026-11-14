# 2026-11-14 — A node session is not listed its stack reads by its own host when its stack is on another host

**Category:** Future enhancement
**Source:** `#orchestrators` 13/13 (stack-node-reads), `Boundaries` of its changeset

A session bound to a node of a PR stack is served `pr_stack_status` and `pr_read` by
the host that holds that stack, even when the session itself is held on another host:
the holding host checks its own store for a node bound to the caller. But the session's
**own** host does not *list* the two reads in its `ListExecTools` answer. Its
classification reads the stack store on its own host, and a stack held elsewhere has no
record there, so the agent is never shown the tools it could use by addressing the call
at the holding host.

**Context:** the developer accepted cross-host gaps of this shape earlier on
2026-11-14 (recorded in the forwarded-caller backlog entry for the conversation
spawner's start tools, whose stack-node section this node closed). This listing gap is
the remaining half of that shape, kept out of 13/13's boundaries deliberately: no
planned scope is deferred by it.

## What would close it

Carry the node binding to the session's own host — for example record the binding (or
the holding host and stack id) on the session at spawn, or have `ListExecTools` ask the
peers that could hold the stack — so a node session is listed its two reads wherever it
runs, with a test mirroring
`a_stack_node_session_in_a_jail_is_listed_its_two_reads_as_the_daemon_lists_them` for
the cross-host case.

## Developer decision

**Deferred by the developer at the #orchestrators TODO gate, 2026-11-14.** Cross-host listing gaps were accepted earlier in the stack; this one stays open until a peer lookup of node bindings is wanted.
