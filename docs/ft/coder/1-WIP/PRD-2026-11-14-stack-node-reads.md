# PRD: a stack node's session reads its stack in a jail and from another host

**Date**: 2026-11-14
**Status**: 🚧 Planned (`#orchestrators` 13/13)
**Feature doc**: [PR stacking § PR-management tools](../pr-stacking.md)
**Changeset**: [2026-11-14-stack-node-reads.md](../../../dev/1-WIP/2026-11-14-stack-node-reads.md)

## Problem

`#orchestrators` 7/11 (#884) offers a session bound to a node of a PR stack two read tools for that
stack: `pr_stack_status` and `pr_read`. Two kinds of node session cannot use them today:

1. **A node session in a jail.** Its agent learns its tools from the jail's own `ListExecTools`
   bridge, which never lists the two reads. The daemon would serve them, but the agent never sees
   them.
2. **A node session on another host.** When its call is forwarded to the host that holds its stack,
   that host confirms the caller with the session's owner. It accepts only an orchestrator role, so
   it refuses the node.

Either way, the agent working a node cannot read its own stack's status or its pull requests. It
has to ask its orchestrator.

## Requirements

- **R1.** A session in a jail is listed the same stack tools that the daemon lists it: the two reads
  for a node session, and everything an orchestrator is offered.
- **R2.** A host that holds a stack serves the two reads to a caller held on another host when a node
  of that stack is bound to the caller. The caller's owner must still confirm that it holds the
  session.
- **R3.** Nothing else widens. A node session on another host is still refused every stack write, as
  permission denied. A session bound to no node of the host's stacks is still refused the reads. The
  stack git tools are still served only to orchestrator-role callers.

## Out of scope

- A node session whose orchestrator and stack live on **another** host is not listed the two reads
  by its own host's `ListExecTools`. Its own host cannot see that stack's records. This node does
  not change that listing.
- A conversation spawner (a grill-me session) on another host is still refused the start tools. That
  is the other half of
  [`2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md`](../../../dev/todo/2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md).

## Acceptance

- A node session in a jail is listed `pr_stack_status` and `pr_read`, exactly what the daemon lists
  it.
- An orchestrator in a jail is listed all eleven stack tools, exactly what the daemon lists it.
- A node session on host-b reads the status of its stack on host-a by addressing the call to host-a.
  Its `pr_add_planned` there is refused as permission denied, and the stack is unchanged.
- A session on host-b that no node on host-a is bound to is refused `pr_stack_status` there.
