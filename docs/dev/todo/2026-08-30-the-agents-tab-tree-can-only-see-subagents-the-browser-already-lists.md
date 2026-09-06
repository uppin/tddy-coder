# 2026-08-30 — The Agents tab tree can only see subagents the browser already lists

**Category:** Future enhancement
**Source:** agents-tab-subagent-tree changeset, 2026-08-30

- The tree's non-managed branch is folded from the drawer's `ListSessions` list by
  `orchestrator_session_id`. A subagent session spawned on a host the browser is not aggregating —
  or one whose orchestrator row is missing from the list — is dropped rather than shown, because an
  orphan promoted to the root would claim the main agent spawned it (`agentTree.ts`).
- The operator therefore sees a *complete* tree only when the session list is complete. Nothing on
  screen says the tree may be partial, and there is no signal on the wire that would let it: a
  `SessionEntry` says who spawned it, never how many it spawned.
- A fix wants a child count on the parent — `SessionEntry.subagent_count`, stamped by the daemon that
  owns the parent — so a node can say "3 subagents, 1 not listed here" instead of quietly rendering
  two. That is a proto and daemon change, which is why it is not in the web-only changeset.
